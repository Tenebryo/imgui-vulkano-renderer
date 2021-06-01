mod shader;

use vulkano::{buffer::{BufferAccess, BufferUsage, CpuBufferPool}, command_buffer::{PrimaryAutoCommandBuffer, SubpassContents}, image::{ImageDimensions, ImageViewAbstract, view::ImageView}, render_pass::RenderPass};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;
use vulkano::descriptor::PipelineLayoutAbstract;
use vulkano::device::{Device, Queue};
use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};
use vulkano::sync::GpuFuture;

use vulkano::image::ImmutableImage;
use vulkano::sampler::Sampler;
// use vulkano::sampler::{Sampler, SamplerAddressMode, Filter, MipmapMode};
use vulkano::format::{Format, ClearValue};
use vulkano::render_pass::Subpass;
use vulkano::render_pass::Framebuffer;
use vulkano::pipeline::viewport::Scissor;
use vulkano::pipeline::viewport::Viewport;

use std::sync::Arc;
use std::fmt;

use imgui::{DrawVert, Textures, DrawCmd, DrawCmdParams, internal::RawWrapper, TextureId, ImString};

#[derive(Default, Debug, Clone)]
#[repr(C)]
struct Vertex {
    pub pos: [f32; 2],
    pub uv : [f32; 2],
    pub col: u32,
    // pub col: [u8; 4],
}

vulkano::impl_vertex!(Vertex, pos, uv, col);

impl From<DrawVert> for Vertex {
    fn from(v : DrawVert) -> Vertex {
        unsafe{std::mem::transmute(v)}
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
            },
            &Self::BadImageDimensions(d) => {
                write!(f, "Image Dimensions not supported (must be Dim2d): {:?}", d)
            },
        }
    }
}

impl std::error::Error for RendererError {}


pub type Texture = (Arc<dyn ImageViewAbstract + Send + Sync>, Arc<Sampler>);

pub struct Renderer {
    render_pass : Arc<RenderPass>,
    pipeline : Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
    font_texture : Texture,
    textures : Textures<Texture>,
    vrt_buffer_pool : CpuBufferPool<Vertex>,
    idx_buffer_pool : CpuBufferPool<u16>,
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
    pub fn init(ctx: &mut imgui::Context, device : Arc<Device>, queue : Arc<Queue>, format : Format) -> Result<Renderer, Box<dyn std::error::Error>> {

        let vs = shader::vs::Shader::load(device.clone()).unwrap();
        let fs = shader::fs::Shader::load(device.clone()).unwrap();

        let render_pass = Arc::new(
            vulkano::single_pass_renderpass!(
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
            .unwrap(),
        );

        let pipeline = Arc::new(GraphicsPipeline::start()
            .vertex_input_single_buffer::<Vertex>()
            .vertex_shader(vs.main_entry_point(), ())
            .triangle_list()
            .viewports_scissors_dynamic(1)
            .fragment_shader(fs.main_entry_point(), ())
            .blend_alpha_blending()
            .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
            .build(device.clone())?);


        let textures = Textures::new();

        let font_texture = Self::upload_font_texture(ctx.fonts(), device.clone(), queue.clone())?;


        ctx.set_renderer_name(Some(ImString::from(format!("imgui-vulkano-renderer {}", env!("CARGO_PKG_VERSION")))));

        let vrt_buffer_pool = CpuBufferPool::new(device.clone(), BufferUsage::vertex_buffer_transfer_destination());
        let idx_buffer_pool = CpuBufferPool::new(device.clone(), BufferUsage::index_buffer_transfer_destination());

        Ok(Renderer {
            render_pass,
            pipeline : pipeline as Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
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
    pub fn draw_commands<I>(&mut self, cmd_buf_builder : &mut AutoCommandBufferBuilder<PrimaryAutoCommandBuffer>, _queue : Arc<Queue>, target : I, draw_data : &imgui::DrawData) -> Result<(), Box<dyn std::error::Error>> 
    where I: ImageViewAbstract + Send + Sync + 'static {

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
            matrix : [
                [(2.0 / (right - left)), 0.0, 0.0, 0.0],
                [0.0, (2.0 / (bottom - top)), 0.0, 0.0],
                [0.0, 0.0, -1.0, 0.0],
                [
                    (right + left) / (left - right),
                    (top + bottom) / (top - bottom),
                    0.0,
                    1.0,
                ],
            ]
        };

        let dims = match target.image().dimensions() {
            ImageDimensions::Dim2d {width, height, ..} => {[width, height]},
            d => { return Err(Box::new(RendererError::BadImageDimensions(d)));}
        };

        let mut dynamic_state = DynamicState::default();
        dynamic_state.viewports = Some(vec![
            Viewport {
                origin: [0.0, 0.0],
                dimensions: [dims[0] as f32, dims[1] as f32],
                depth_range: 0.0 .. 1.0,
            }
        ]);
        dynamic_state.scissors = Some(vec![
            Scissor::default()
        ]);

        let clip_off = draw_data.display_pos;
        let clip_scale = draw_data.framebuffer_scale;

        
        let layout = self.pipeline.descriptor_set_layout(0).unwrap();

        let framebuffer = Arc::new(Framebuffer::start(self.render_pass.clone())
            .add(target)?.build()?);

        cmd_buf_builder.begin_render_pass(framebuffer, SubpassContents::Inline, vec![ClearValue::None])?;

        for draw_list in draw_data.draw_lists() {
            
            let vertex_buffer = Arc::new(self.vrt_buffer_pool.chunk(draw_list.vtx_buffer().iter().map(|&v| Vertex::from(v))).unwrap());
            let index_buffer  = Arc::new(self.idx_buffer_pool.chunk(draw_list.idx_buffer().iter().cloned()).unwrap());

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

                            if let Some(ref mut scissors) = dynamic_state.scissors {
                                scissors[0] = Scissor {
                                    origin: [
                                        f32::max(0.0, clip_rect[0]).floor() as i32,
                                        f32::max(0.0, clip_rect[1]).floor() as i32
                                    ],
                                    dimensions: [
                                        (clip_rect[2] - clip_rect[0]).abs().ceil() as u32,
                                        (clip_rect[3] - clip_rect[1]).abs().ceil() as u32
                                    ],
                                };
                            }

                            let tex = self.lookup_texture(texture_id)?;

                            let set = Arc::new(PersistentDescriptorSet::start(layout.clone())
                                .add_sampled_image(tex.0.clone(), tex.1.clone())?
                                .build()?
                            );

                            cmd_buf_builder.draw_indexed(
                                self.pipeline.clone(), 
                                &dynamic_state, 
                                vec![vertex_buffer.clone()], 
                                index_buffer.clone().into_buffer_slice().slice(idx_offset..(idx_offset+count)).unwrap(),
                                set,
                                pc,
                                vec![])?;
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
        device : Arc<Device>,
        queue : Arc<Queue>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.font_texture = Self::upload_font_texture(ctx.fonts(), device, queue)?;
        Ok(())
    }

    /// Get the texture library that the renderer uses
    pub fn textures(&mut self) -> &mut Textures<Texture> {
        &mut self.textures
    }

    fn upload_font_texture(
        mut fonts: imgui::FontAtlasRefMut,
        device : Arc<Device>,
        queue : Arc<Queue>,
    ) -> Result<Texture, Box<dyn std::error::Error>> {
        let texture = fonts.build_rgba32_texture();

        let (image, fut) = ImmutableImage::from_iter(
            texture.data.iter().cloned(),
            ImageDimensions::Dim2d{
                width : texture.width,
                height : texture.height,
                array_layers : 1,
            },
            vulkano::image::MipmapsCount::One,
            Format::R8G8B8A8Srgb,
            queue.clone(),
            )?;

        fut.then_signal_fence_and_flush()?
            .wait(None)?;

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