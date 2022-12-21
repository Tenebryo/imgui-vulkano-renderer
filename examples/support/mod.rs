use imgui::{Context, FontConfig, FontGlyphRanges, FontSource, Ui};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::time::{Duration, Instant};
use vulkano::{
    command_buffer::allocator::StandardCommandBufferAllocator,
    command_buffer::allocator::StandardCommandBufferAllocatorCreateInfo,
    command_buffer::AutoCommandBufferBuilder,
    command_buffer::ClearColorImageInfo,
    device::physical::PhysicalDeviceType,
    device::DeviceCreateInfo,
    device::Queue,
    device::QueueCreateInfo,
    device::{Device, DeviceExtensions},
    image::view::ImageView,
    image::{ImageUsage, SwapchainImage},
    instance::Instance,
    instance::InstanceCreateInfo,
    memory::allocator::StandardMemoryAllocator,
    swapchain,
    swapchain::Surface,
    swapchain::SwapchainCreateInfo,
    swapchain::{AcquireError, ColorSpace, Swapchain, SwapchainCreationError},
    sync,
    sync::{FlushError, GpuFuture},
    VulkanLibrary,
};

use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use vulkano_win::VkSurfaceBuild;

use std::sync::Arc;

use imgui_vulkano_renderer::Renderer;

mod clipboard;

pub struct System {
    pub event_loop: EventLoop<()>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub surface: Arc<Surface>,
    pub swapchain: Arc<Swapchain>,
    pub images: Vec<Arc<SwapchainImage>>,
    pub imgui: Context,
    pub platform: WinitPlatform,
    pub renderer: Renderer,
    pub font_size: f32,

    pub memory_allocator: Arc<StandardMemoryAllocator>,
    pub command_buffer_allocator: StandardCommandBufferAllocator,
}

pub fn init(title: &str) -> System {
    let library = VulkanLibrary::new().unwrap();

    let required_extensions = vulkano_win::required_extensions(&library);
    let instance = Instance::new(
        library,
        InstanceCreateInfo {
            enabled_extensions: required_extensions,
            ..Default::default()
        },
    )
    .unwrap();

    let device_extensions = DeviceExtensions {
        khr_swapchain: true,
        ..DeviceExtensions::empty()
    };

    let title = match title.rfind('/') {
        Some(idx) => title.split_at(idx + 1).1,
        None => title,
    };

    let event_loop = EventLoop::new();
    let surface = WindowBuilder::new()
        .with_title(title.to_owned())
        .build_vk_surface(&event_loop, instance.clone())
        .expect("Failed to create a window!");
    let window = surface.object().unwrap().downcast_ref::<Window>().unwrap();

    let (physical, queue_family) = instance
        .enumerate_physical_devices()
        .unwrap()
        .filter(|p| p.supported_extensions().contains(&device_extensions))
        .filter_map(|p| {
            let queue_family = p
                .queue_family_properties()
                .iter()
                .enumerate()
                .find(|(i, q)| {
                    q.queue_flags.graphics
                        && p.surface_support(*i as u32, &surface).unwrap_or(false)
                })
                .map(|(i, _q)| i);

            queue_family.map(|i| (p, i))
        })
        .min_by_key(|(p, _)| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            _ => 4,
        })
        .unwrap();

    let (device, mut queues) = Device::new(
        Arc::clone(&physical),
        DeviceCreateInfo {
            enabled_extensions: device_extensions,
            queue_create_infos: vec![QueueCreateInfo {
                queue_family_index: queue_family as u32,
                ..Default::default()
            }],
            ..Default::default()
        },
    )
    .unwrap();

    let queue = queues.next().unwrap();

    let format;

    let (swapchain, images) = {
        let caps = physical
            .surface_capabilities(&surface, Default::default())
            .unwrap();

        format = Some(
            physical
                .surface_formats(&surface, Default::default())
                .unwrap()[0]
                .0,
        );

        let image_usage = ImageUsage {
            transfer_dst: true,
            color_attachment: true,
            ..ImageUsage::empty()
        };

        Swapchain::new(
            device.clone(),
            surface.clone(),
            SwapchainCreateInfo {
                min_image_count: caps.min_image_count,
                image_format: format,
                image_extent: window.inner_size().into(),
                image_usage,
                composite_alpha: caps.supported_composite_alpha.iter().next().unwrap(),
                image_color_space: ColorSpace::SrgbNonLinear,
                ..Default::default()
            },
        )
        .unwrap()
    };

    let mut imgui = Context::create();
    imgui.set_ini_filename(None);

    if let Some(backend) = clipboard::init() {
        imgui.set_clipboard_backend(backend);
    } else {
        eprintln!("Failed to initialize clipboard");
    }

    let mut platform = WinitPlatform::init(&mut imgui);
    platform.attach_window(imgui.io_mut(), window, HiDpiMode::Rounded);

    let hidpi_factor = platform.hidpi_factor();
    let font_size = (13.0 * hidpi_factor) as f32;
    imgui.fonts().add_font(&[
        FontSource::DefaultFontData {
            config: Some(FontConfig {
                size_pixels: font_size,
                ..FontConfig::default()
            }),
        },
        FontSource::TtfData {
            data: include_bytes!("../resources/mplus-1p-regular.ttf"),
            size_pixels: font_size,
            config: Some(FontConfig {
                rasterizer_multiply: 1.75,
                glyph_ranges: FontGlyphRanges::japanese(),
                ..FontConfig::default()
            }),
        },
    ]);

    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

    let memory_allocator = Arc::new(StandardMemoryAllocator::new_default(Arc::clone(&device)));

    let command_buffer_allocator = StandardCommandBufferAllocator::new(
        Arc::clone(&device),
        StandardCommandBufferAllocatorCreateInfo::default(),
    );

    let renderer = Renderer::init(
        &mut imgui,
        device.clone(),
        queue.clone(),
        format.unwrap(),
        None,
        None,
    )
    .expect("Failed to initialize renderer");

    System {
        event_loop,
        device,
        queue,
        surface,
        swapchain,
        images,
        imgui,
        platform,
        renderer,
        font_size,

        memory_allocator,
        command_buffer_allocator,
    }
}

impl System {
    pub fn main_loop<F: FnMut(&mut bool, &mut Ui) + 'static>(self, mut run_ui: F) {
        let System {
            event_loop,
            device,
            queue,
            surface,
            mut swapchain,
            mut images,
            mut imgui,
            mut platform,
            mut renderer,
            ..
        } = self;

        let mut recreate_swapchain = false;

        let mut previous_frame_end = Some(sync::now(device.clone()).boxed());

        let mut last_redraw = Instant::now();

        // target 60 fps
        let target_frame_time = Duration::from_millis(1000 / 60);

        event_loop.run(move |event, _, control_flow| {
            let window = surface.object().unwrap().downcast_ref::<Window>().unwrap();

            platform.handle_event(imgui.io_mut(), &window, &event);
            match event {
                Event::NewEvents(_) => {
                    // imgui.io_mut().update_delta_time(Instant::now());
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(_),
                    ..
                } => {
                    recreate_swapchain = true;
                }
                Event::MainEventsCleared => {
                    platform
                        .prepare_frame(imgui.io_mut(), &window)
                        .expect("Failed to prepare frame");
                    window.request_redraw();
                }
                Event::RedrawEventsCleared => {
                    let t = Instant::now();
                    let since_last = t.duration_since(last_redraw);
                    last_redraw = t;

                    if since_last > target_frame_time {
                        if since_last < target_frame_time {
                            std::thread::sleep(target_frame_time - since_last);
                        }
                    }

                    previous_frame_end.as_mut().unwrap().cleanup_finished();

                    if recreate_swapchain {
                        let (new_swapchain, new_images) =
                            match swapchain.recreate(SwapchainCreateInfo {
                                image_extent: window.inner_size().into(),
                                ..swapchain.create_info()
                            }) {
                                Ok(r) => r,
                                Err(SwapchainCreationError::ImageExtentNotSupported { .. }) => {
                                    return
                                }
                                Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                            };

                        images = new_images;
                        swapchain = new_swapchain;
                        recreate_swapchain = false;
                    }

                    let mut ui = imgui.frame();

                    let mut run = true;
                    run_ui(&mut run, &mut ui);
                    if !run {
                        *control_flow = ControlFlow::Exit;
                    }

                    let (image_num, suboptimal, acquire_future) =
                        match swapchain::acquire_next_image(swapchain.clone(), None) {
                            Ok(r) => r,
                            Err(AcquireError::OutOfDate) => {
                                recreate_swapchain = true;
                                return;
                            }
                            Err(e) => panic!("Failed to acquire next image: {:?}", e),
                        };

                    if suboptimal {
                        recreate_swapchain = true;
                    }

                    platform.prepare_render(&ui, window);

                    let draw_data = imgui.render();

                    let mut cmd_buf_builder = AutoCommandBufferBuilder::primary(
                        &self.command_buffer_allocator,
                        queue.queue_family_index(),
                        vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit,
                    )
                    .expect("Failed to create command buffer");

                    cmd_buf_builder
                        .clear_color_image(ClearColorImageInfo::image(
                            images[image_num as usize].clone(),
                        ))
                        .expect("Failed to create image clear command");

                    renderer
                        .draw_commands(
                            &mut cmd_buf_builder,
                            ImageView::new_default(images[image_num as usize].clone()).unwrap(),
                            draw_data,
                        )
                        .expect("Rendering failed");

                    let cmd_buf = cmd_buf_builder
                        .build()
                        .expect("Failed to build command buffer");

                    let future = previous_frame_end
                        .take()
                        .unwrap()
                        .join(acquire_future)
                        .then_execute(queue.clone(), cmd_buf)
                        .unwrap()
                        .then_signal_fence()
                        .then_swapchain_present(
                            queue.clone(),
                            swapchain::SwapchainPresentInfo::swapchain_image_index(
                                Arc::clone(&swapchain),
                                image_num,
                            ),
                        );

                    match future.flush() {
                        Ok(_) => {
                            previous_frame_end = Some(future.boxed());
                        }
                        Err(FlushError::OutOfDate) => {
                            recreate_swapchain = true;
                            previous_frame_end = Some(sync::now(device.clone()).boxed());
                        }
                        Err(e) => {
                            println!("Failed to flush future: {:?}", e);
                            previous_frame_end = Some(sync::now(device.clone()).boxed());
                        }
                    }
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => *control_flow = ControlFlow::Exit,
                event => {
                    platform.handle_event(imgui.io_mut(), window, &event);
                }
            }
        })
    }
}
