mod cache;
mod shader;

use cache::DescriptorSetCache;

use bytemuck::{Pod, Zeroable};
use vulkano::{
    buffer::{BufferUsage, CpuBufferPool},
    command_buffer::{
        allocator::{StandardCommandBufferAllocator, StandardCommandBufferAllocatorCreateInfo},
        AutoCommandBufferBuilder, CommandBufferUsage, PrimaryCommandBufferAbstract,
    },
    command_buffer::{PrimaryAutoCommandBuffer, SubpassContents},
    descriptor_set::{
        allocator::StandardDescriptorSetAllocator, PersistentDescriptorSet, WriteDescriptorSet,
    },
    device::{Device, Queue},
    format::Format,
    image::ImmutableImage,
    image::{view::ImageView, ImageDimensions, ImageViewAbstract},
    memory::allocator::{MemoryUsage, StandardMemoryAllocator},
    pipeline::graphics::color_blend::ColorBlendState,
    pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology},
    pipeline::graphics::vertex_input::BuffersDefinition,
    pipeline::graphics::viewport::{Scissor, Viewport, ViewportState},
    pipeline::{GraphicsPipeline, Pipeline},
    render_pass::RenderPass,
    render_pass::Subpass,
    render_pass::{Framebuffer, FramebufferCreateInfo},
    sampler::{Sampler, SamplerCreateInfo},
    sync::GpuFuture,
};

use std::{convert::TryFrom, fmt, sync::Arc};

use imgui::{internal::RawWrapper, DrawCmd, DrawCmdParams, DrawVert, TextureId, Textures};

#[derive(Default, Debug, Copy, Clone, Zeroable, Pod)]
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

pub struct Allocators {
    pub descriptor_sets: Arc<StandardDescriptorSetAllocator>,
    pub memory: Arc<StandardMemoryAllocator>,
    pub command_buffers: Arc<StandardCommandBufferAllocator>,
}

pub struct Renderer {
    render_pass: Arc<RenderPass>,
    pipeline: Arc<GraphicsPipeline>,
    font_texture: Texture,
    textures: Textures<Texture>,
    vrt_buffer_pool: CpuBufferPool<Vertex>,
    idx_buffer_pool: CpuBufferPool<u16>,

    allocators: Allocators,

    descriptor_set_cache: DescriptorSetCache,
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

        gamma: Option<f32>,
        allocators: Option<Allocators>,
    ) -> Result<Renderer, Box<dyn std::error::Error>> {
        let allocators = allocators.unwrap_or_else(|| Allocators {
            descriptor_sets: Arc::new(StandardDescriptorSetAllocator::new(Arc::clone(&device))),
            memory: Arc::new(StandardMemoryAllocator::new_default(Arc::clone(&device))),
            command_buffers: Arc::new(StandardCommandBufferAllocator::new(
                Arc::clone(&device),
                StandardCommandBufferAllocatorCreateInfo::default(),
            )),
        });

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
            .fragment_shader(
                fs.entry_point("main").unwrap(),
                shader::fs::SpecializationConstants {
                    OUT_GAMMA: gamma.unwrap_or(1.0),
                },
            )
            .color_blend_state(ColorBlendState::new(subpass.num_color_attachments()).blend_alpha())
            .render_pass(subpass)
            .build(device.clone())?;

        let textures = Textures::new();

        let font_texture = Self::upload_font_texture(
            &mut ctx.fonts(),
            device.clone(),
            queue.clone(),
            &allocators,
        )?;

        ctx.set_renderer_name(Some(format!(
            "imgui-vulkano-renderer {}",
            env!("CARGO_PKG_VERSION")
        )));

        let vrt_buffer_pool = CpuBufferPool::new(
            Arc::clone(&allocators.memory),
            BufferUsage {
                vertex_buffer: true,
                transfer_dst: true,
                ..BufferUsage::empty()
            },
            vulkano::memory::allocator::MemoryUsage::Upload,
        );
        let idx_buffer_pool = CpuBufferPool::new(
            Arc::clone(&allocators.memory),
            BufferUsage {
                transfer_dst: true,
                index_buffer: true,
                ..BufferUsage::empty()
            },
            MemoryUsage::Upload,
        );

        Ok(Renderer {
            render_pass,
            pipeline,
            font_texture,
            textures,
            vrt_buffer_pool,
            idx_buffer_pool,
            allocators,

            descriptor_set_cache: DescriptorSetCache::default(),
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

        let layout = self.pipeline.layout().set_layouts().get(0).unwrap();

        // Creating a new Framebuffer every frame isn't ideal, but according to this thread,
        // it also isn't really an issue on desktop GPUs:
        // https://github.com/GameTechDev/IntroductionToVulkan/issues/20
        // This might be a good target for optimizations in the future though.
        let framebuffer = Framebuffer::new(
            self.render_pass.clone(),
            FramebufferCreateInfo {
                attachments: vec![target],
                ..Default::default()
            },
        )?;

        let mut info = vulkano::command_buffer::RenderPassBeginInfo::framebuffer(framebuffer);
        info.clear_values = vec![Some([0.0].into())];

        cmd_buf_builder
            .begin_render_pass(info, SubpassContents::Inline)?
            .bind_pipeline_graphics(self.pipeline.clone());

        for draw_list in draw_data.draw_lists() {
            let vertex_buffer = self
                .vrt_buffer_pool
                .from_iter(draw_list.vtx_buffer().iter().map(|&v| Vertex::from(v)))
                .unwrap();
            let index_buffer = self
                .idx_buffer_pool
                .from_iter(draw_list.idx_buffer().iter().cloned())
                .unwrap();

            for cmd in draw_list.commands() {
                match cmd {
                    DrawCmd::Elements {
                        count,
                        cmd_params:
                            DrawCmdParams {
                                clip_rect,
                                texture_id,
                                idx_offset,
                                // vtx_offset,
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
                            let set = self.descriptor_set_cache.get_or_insert(
                                texture_id,
                                |texture_id| {
                                    let (img, sampler) = Self::lookup_texture(
                                        &self.textures,
                                        &self.font_texture,
                                        texture_id,
                                    )?
                                    .clone();
                                    Ok(PersistentDescriptorSet::new(
                                        &*self.allocators.descriptor_sets,
                                        layout.clone(),
                                        [WriteDescriptorSet::image_view_sampler(0, img, sampler)],
                                    )?)
                                },
                            )?;

                            cmd_buf_builder
                                .bind_descriptor_sets(
                                    vulkano::pipeline::PipelineBindPoint::Graphics,
                                    self.pipeline.layout().clone(),
                                    0,
                                    set,
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
        self.descriptor_set_cache.clear_font_texture();
        self.font_texture =
            Self::upload_font_texture(&mut ctx.fonts(), device, queue, &self.allocators)?;
        Ok(())
    }

    /// Get the texture library that the renderer uses
    pub fn textures_mut(&mut self) -> &mut Textures<Texture> {
        // make sure to recreate descriptors if necessary
        self.descriptor_set_cache.clear();
        &mut self.textures
    }

    /// Get the texture library that the renderer uses
    pub fn textures(&self) -> &Textures<Texture> {
        &self.textures
    }

    fn upload_font_texture(
        fonts: &mut imgui::FontAtlas,
        device: Arc<Device>,
        queue: Arc<Queue>,
        allocators: &Allocators,
    ) -> Result<Texture, Box<dyn std::error::Error>> {
        let texture = fonts.build_rgba32_texture();

        let mut builder = AutoCommandBufferBuilder::primary(
            &*allocators.command_buffers,
            queue.queue_family_index(),
            CommandBufferUsage::OneTimeSubmit,
        )?;

        let image = ImmutableImage::from_iter(
            &*allocators.memory,
            texture.data.iter().cloned(),
            ImageDimensions::Dim2d {
                width: texture.width,
                height: texture.height,
                array_layers: 1,
            },
            vulkano::image::MipmapsCount::One,
            Format::R8G8B8A8_SRGB,
            &mut builder,
        )?;

        let command_buffer = builder.build()?;

        command_buffer
            .execute(queue)?
            .then_signal_fence_and_flush()?
            .wait(None)?;

        let sampler = Sampler::new(device.clone(), SamplerCreateInfo::simple_repeat_linear())?;

        fonts.tex_id = TextureId::from(usize::MAX);
        Ok((ImageView::new_default(image)?, sampler))
    }

    fn lookup_texture<'a>(
        textures: &'a Textures<Texture>,
        font_texture: &'a Texture,
        texture_id: TextureId,
    ) -> Result<&'a Texture, RendererError> {
        if texture_id.id() == usize::MAX {
            Ok(&font_texture)
        } else if let Some(texture) = textures.get(texture_id) {
            Ok(texture)
        } else {
            Err(RendererError::BadTexture(texture_id))
        }
    }
}
