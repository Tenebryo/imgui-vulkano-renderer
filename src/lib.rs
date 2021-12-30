mod shader;

use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::pipeline::graphics::color_blend::ColorBlendState;
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::vertex_input::BuffersDefinition;
use vulkano::pipeline::graphics::viewport::{Scissor, Viewport, ViewportState};
use vulkano::{
    buffer::{BufferUsage, CpuBufferPool},
    command_buffer::{PrimaryAutoCommandBuffer, SubpassContents},
    image::{view::ImageView, ImageDimensions, ImageViewAbstract},
    render_pass::RenderPass,
};

use vulkano::device::{Device, Queue};
use vulkano::pipeline::{GraphicsPipeline, Pipeline};
use vulkano::sync::GpuFuture;

use vulkano::image::ImmutableImage;
use vulkano::sampler::Sampler;
// use vulkano::sampler::{Sampler, SamplerAddressMode, Filter, MipmapMode};
use vulkano::format::{ClearValue, Format};

use vulkano::render_pass::Framebuffer;
use vulkano::render_pass::Subpass;

use std::fmt;
use std::sync::Arc;

use imgui::{internal::RawWrapper, DrawCmd, DrawCmdParams, DrawVert, TextureId, Textures};

#[derive(Default, Debug, Clone)]
#[repr(C)]
struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub col: u32,
    // pub col: [u8; 4],
}

vulkano::impl_vertex!(Vertex, pos, uv, col);

impl From<DrawVert> for Vertex {
    fn from(v: DrawVert) -> Vertex {
        unsafe { std::mem::transmute(v) }
    }
}

#[derive(Debug)]
pub enum RendererError {
    BadTexture(TextureId),
    BadImageDimensions(ImageDimensions),
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            &Self::BadTexture(ref t) => {
                write!(f, "The Texture ID could not be found: {:?}", t)
            }
            &Self::BadImageDimensions(d) => {
                write!(f, "Image Dimensions not supported (must be Dim2d): {:?}", d)
            }
        }
    }
}

impl std::error::Error for RendererError {}

pub type Texture = (Arc<dyn ImageViewAbstract + Send + Sync>, Arc<Sampler>);

pub struct Renderer {
    render_pass: Arc<RenderPass>,
    pipeline: Arc<GraphicsPipeline>,
    font_texture: Texture,
    textures: Textures<Texture>,
    vrt_buffer_pool: CpuBufferPool<Vertex>,
    idx_buffer_pool: CpuBufferPool<u16>,
}

impl Renderer {
    /// Initialize the renderer object, including vertex buffers, ImGui font textures,
    /// and the Vulkan graphics pipeline.
    ///
    /// ---
    ///
    /// `ctx`: the ImGui `Context` object
    ///
    /// `device`: the Vulkano `Device` object for the device you want to render the UI on.
    ///
    /// `queue`: the Vulkano `Queue` object for the queue the font atlas texture will be created on.
    ///
    /// `format`: the Vulkano `Format` that the render pass will use when storing the frame in the target image.
    pub fn init(
        ctx: &mut imgui::Context,
        device: Arc<Device>,
        queue: Arc<Queue>,
        format: Format,
    ) -> Result<Renderer, Box<dyn std::error::Error>> {
        let vs = shader::vs::load(device.clone()).unwrap();
        let fs = shader::fs::load(device.clone()).unwrap();

        let render_pass = vulkano::single_pass_renderpass!(
            device.clone(),
            attachments: {
                color: {
                    load: Load,
                    store: Store,
                    format: format,
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {}
            }
        )
        .unwrap();
        let subpass = Subpass::from(render_pass.clone(), 0).unwrap();
        let pipeline = GraphicsPipeline::start()
            .vertex_input_state(BuffersDefinition::new().vertex::<Vertex>())
            .vertex_shader(vs.entry_point("main").unwrap(), ())
            .input_assembly_state(
                InputAssemblyState::new().topology(PrimitiveTopology::TriangleList),
            )
            .viewport_state(ViewportState::viewport_dynamic_scissor_dynamic(1))
            .fragment_shader(fs.entry_point("main").unwrap(), ())
            .color_blend_state(ColorBlendState::new(subpass.num_color_attachments()).blend_alpha())
            .render_pass(subpass)
            .build(device.clone())?;

        let textures = Textures::new();

        let font_texture =
            Self::upload_font_texture(&mut ctx.fonts(), device.clone(), queue.clone())?;

        ctx.set_renderer_name(Some(format!(
            "imgui-vulkano-renderer {}",
            env!("CARGO_PKG_VERSION")
        )));

        let vrt_buffer_pool = CpuBufferPool::new(
            device.clone(),
            BufferUsage::vertex_buffer_transfer_destination(),
        );
        let idx_buffer_pool = CpuBufferPool::new(
            device.clone(),
            BufferUsage::index_buffer_transfer_destination(),
        );

        Ok(Renderer {
            render_pass,
            pipeline,
            font_texture,
            textures,
            vrt_buffer_pool,
            idx_buffer_pool,
        })
    }

    /// Appends the draw commands for the UI frame to an `AutoCommandBufferBuilder`.
    ///
    /// ---
    ///
    /// `cmd_buf_builder`: An `AutoCommandBufferBuilder` from vulkano to add commands to
    ///
    /// `device`: the Vulkano `Device` object for the device you want to render the UI on
    ///
    /// `queue`: the Vulkano `Queue` object for buffer creation
    ///
    /// `target`: the target image to render to
    ///
    /// `draw_data`: the ImGui `DrawData` that each UI frame creates
    pub fn draw_commands<I>(
        &mut self,
        cmd_buf_builder: &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>,
        _queue: Arc<Queue>,
        target: Arc<I>,
        draw_data: &imgui::DrawData,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        I: ImageViewAbstract + Send + Sync + 'static,
    {
        let fb_width = draw_data.display_size[0] * draw_data.framebuffer_scale[0];
        let fb_height = draw_data.display_size[1] * draw_data.framebuffer_scale[1];
        if !(fb_width > 0.0 && fb_height > 0.0) {
            return Ok(());
        }
        let left = draw_data.display_pos[0];
        let right = draw_data.display_pos[0] + draw_data.display_size[0];
        let top = draw_data.display_pos[1];
        let bottom = draw_data.display_pos[1] + draw_data.display_size[1];

        let pc = shader::vs::ty::VertPC {
            matrix: [
                [(2.0 / (right - left)), 0.0, 0.0, 0.0],
                [0.0, (2.0 / (bottom - top)), 0.0, 0.0],
                [0.0, 0.0, -1.0, 0.0],
                [
                    (right + left) / (left - right),
                    (top + bottom) / (top - bottom),
                    0.0,
                    1.0,
                ],
            ],
        };

        let dims = match target.image().dimensions() {
            ImageDimensions::Dim2d { width, height, .. } => [width, height],
            d => {
                return Err(Box::new(RendererError::BadImageDimensions(d)));
            }
        };

        let clip_off = draw_data.display_pos;
        let clip_scale = draw_data.framebuffer_scale;

        let layout = self
            .pipeline
            .layout()
            .descriptor_set_layouts()
            .get(0)
            .unwrap();

        let framebuffer = Framebuffer::start(self.render_pass.clone())
            .add(target)?
            .build()?;

        cmd_buf_builder
            .begin_render_pass(framebuffer, SubpassContents::Inline, vec![ClearValue::None])?
            .bind_pipeline_graphics(self.pipeline.clone());

        for draw_list in draw_data.draw_lists() {
            let vertex_buffer = self
                .vrt_buffer_pool
                .chunk(draw_list.vtx_buffer().iter().map(|&v| Vertex::from(v)))
                .unwrap();
            let index_buffer = self
                .idx_buffer_pool
                .chunk(draw_list.idx_buffer().iter().cloned())
                .unwrap();

            for cmd in draw_list.commands() {
                match cmd {
                    DrawCmd::Elements {
                        count,
                        cmd_params:
                            DrawCmdParams {
                                clip_rect,
                                texture_id,
                                // vtx_offset,
                                idx_offset,
                                ..
                            },
                    } => {
                        let clip_rect = [
                            (clip_rect[0] - clip_off[0]) * clip_scale[0],
                            (clip_rect[1] - clip_off[1]) * clip_scale[1],
                            (clip_rect[2] - clip_off[0]) * clip_scale[0],
                            (clip_rect[3] - clip_off[1]) * clip_scale[1],
                        ];

                        if clip_rect[0] < fb_width
                            && clip_rect[1] < fb_height
                            && clip_rect[2] >= 0.0
                            && clip_rect[3] >= 0.0
                        {
                            let tex = self.lookup_texture(texture_id)?;
                            let set = PersistentDescriptorSet::new(
                                layout.clone(),
                                [WriteDescriptorSet::image_view_sampler(
                                    0,
                                    tex.0.clone(),
                                    tex.1.clone(),
                                )],
                            )?;

                            cmd_buf_builder
                                .bind_descriptor_sets(
                                    vulkano::pipeline::PipelineBindPoint::Graphics,
                                    self.pipeline.layout().clone(),
                                    0,
                                    set.clone(),
                                )
                                .set_scissor(
                                    0,
                                    std::iter::once(Scissor {
                                        origin: [
                                            f32::max(0.0, clip_rect[0]).floor() as u32,
                                            f32::max(0.0, clip_rect[1]).floor() as u32,
                                        ],
                                        dimensions: [
                                            (clip_rect[2] - clip_rect[0]).abs().ceil() as u32,
                                            (clip_rect[3] - clip_rect[1]).abs().ceil() as u32,
                                        ],
                                    }),
                                )
                                .set_viewport(
                                    0,
                                    std::iter::once(Viewport {
                                        origin: [0.0, 0.0],
                                        dimensions: [dims[0] as f32, dims[1] as f32],
                                        depth_range: 0.0..1.0,
                                    }),
                                )
                                .bind_vertex_buffers(0, vertex_buffer.clone())
                                .bind_index_buffer(index_buffer.clone())
                                .push_constants(self.pipeline.layout().clone(), 0, pc)
                                .draw_indexed(count as u32, 1, idx_offset as u32, 0, 0)?;
                        }
                    }
                    DrawCmd::ResetRenderState => (), // TODO
                    DrawCmd::RawCallback { callback, raw_cmd } => unsafe {
                        callback(draw_list.raw(), raw_cmd)
                    },
                }
            }
        }
        cmd_buf_builder.end_render_pass()?;

        Ok(())
    }

    /// Update the ImGui font atlas texture.
    ///
    /// ---
    ///
    /// `ctx`: the ImGui `Context` object
    ///
    /// `device`: the Vulkano `Device` object for the device you want to render the UI on.
    ///
    /// `queue`: the Vulkano `Queue` object for the queue the font atlas texture will be created on.
    pub fn reload_font_texture(
        &mut self,
        ctx: &mut imgui::Context,
        device: Arc<Device>,
        queue: Arc<Queue>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.font_texture = Self::upload_font_texture(&mut ctx.fonts(), device, queue)?;
        Ok(())
    }

    /// Get the texture library that the renderer uses
    pub fn textures(&mut self) -> &mut Textures<Texture> {
        &mut self.textures
    }

    fn upload_font_texture(
        fonts: &mut imgui::FontAtlas,
        device: Arc<Device>,
        queue: Arc<Queue>,
    ) -> Result<Texture, Box<dyn std::error::Error>> {
        let texture = fonts.build_rgba32_texture();

        let (image, fut) = ImmutableImage::from_iter(
            texture.data.iter().cloned(),
            ImageDimensions::Dim2d {
                width: texture.width,
                height: texture.height,
                array_layers: 1,
            },
            vulkano::image::MipmapsCount::One,
            Format::R8G8B8A8_SRGB,
            queue.clone(),
        )?;

        fut.then_signal_fence_and_flush()?.wait(None)?;

        let sampler = Sampler::simple_repeat_linear(device.clone());

        fonts.tex_id = TextureId::from(usize::MAX);
        Ok((ImageView::new(image)?, sampler))
    }

    fn lookup_texture(&self, texture_id: TextureId) -> Result<&Texture, RendererError> {
        if texture_id.id() == usize::MAX {
            Ok(&self.font_texture)
        } else if let Some(texture) = self.textures.get(texture_id) {
            Ok(texture)
        } else {
            Err(RendererError::BadTexture(texture_id))
        }
    }
}
