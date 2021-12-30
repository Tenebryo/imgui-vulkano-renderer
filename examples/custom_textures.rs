use std::error::Error;
use std::io::Cursor;

use vulkano::device::{Device, Queue};
use vulkano::format::Format;
use vulkano::image::view::ImageView;
use vulkano::image::{ImageDimensions, ImmutableImage, MipmapsCount};
use vulkano::sampler::Sampler;
use vulkano::sync::GpuFuture;

use image::{jpeg::JpegDecoder, ImageDecoder};
use imgui::*;
use imgui_vulkano_renderer::Texture;

use std::sync::Arc;

mod support;

#[derive(Default)]
struct CustomTexturesApp {
    my_texture_id: Option<TextureId>,
    lenna: Option<Lenna>,
}

struct Lenna {
    texture_id: TextureId,
    size: [f32; 2],
}

impl CustomTexturesApp {
    fn register_textures(
        &mut self,
        device: Arc<Device>,
        queue: Arc<Queue>,
        textures: &mut Textures<Texture>,
    ) -> Result<(), Box<dyn Error>> {
        const WIDTH: usize = 100;
        const HEIGHT: usize = 100;

        if self.my_texture_id.is_none() {
            // Generate dummy texture
            let mut data = Vec::with_capacity(WIDTH * HEIGHT);
            for i in 0..WIDTH {
                for j in 0..HEIGHT {
                    // Insert RGB values
                    data.push(i as u8);
                    data.push(j as u8);
                    data.push((i + j) as u8);
                    data.push((255) as u8);
                }
            }

            let (texture, fut) = ImmutableImage::from_iter(
                data.iter().cloned(),
                ImageDimensions::Dim2d {
                    width: WIDTH as u32,
                    height: HEIGHT as u32,
                    array_layers: 1,
                },
                MipmapsCount::One,
                Format::R8G8B8A8_SRGB,
                queue.clone(),
            )
            .expect("Failed to create texture");

            let sampler = Sampler::simple_repeat_linear_no_mipmap(device.clone());

            fut.then_signal_fence_and_flush()
                .unwrap()
                .wait(None)
                .expect("Failed to load texture");

            let texture_id = textures.insert((ImageView::new(texture)?, sampler));

            self.my_texture_id = Some(texture_id);
        }

        if self.lenna.is_none() {
            self.lenna = Some(Lenna::new(device, queue, textures)?);
        }

        Ok(())
    }

    fn show_textures(&self, ui: &Ui) {
        Window::new("Hello textures")
            .size([400.0, 600.0], Condition::FirstUseEver)
            .build(ui, || {
                ui.text("Hello textures!");
                if let Some(my_texture_id) = self.my_texture_id {
                    ui.text("Some generated texture");
                    Image::new(my_texture_id, [100.0, 100.0]).build(ui);
                }

                if let Some(lenna) = &self.lenna {
                    ui.text("Say hello to Lenna.jpg");
                    lenna.show(ui);
                }
            });
    }
}

impl Lenna {
    fn new(
        device: Arc<Device>,
        queue: Arc<Queue>,
        textures: &mut Textures<Texture>,
    ) -> Result<Self, Box<dyn Error>> {
        let lenna_bytes = include_bytes!("resources/Lenna.jpg");
        let byte_stream = Cursor::new(lenna_bytes.as_ref());
        let decoder = JpegDecoder::new(byte_stream)?;

        let (width, height) = decoder.dimensions();
        let mut image = vec![0; decoder.total_bytes() as usize];
        decoder.read_image(&mut image)?;

        let mut image_encoded = vec![255u8; (image.len() * 4) / 3];

        for (i, p) in image.chunks_exact(3).enumerate() {
            let j = 4 * i;
            image_encoded[j] = p[0];
            image_encoded[j + 1] = p[1];
            image_encoded[j + 2] = p[2];
        }

        let (texture, fut) = ImmutableImage::from_iter(
            image_encoded.iter().cloned(),
            ImageDimensions::Dim2d {
                width,
                height,
                array_layers: 1,
            },
            MipmapsCount::One,
            Format::R8G8B8A8_SRGB,
            queue.clone(),
        )
        .expect("Failed to create texture");

        let sampler = Sampler::simple_repeat_linear_no_mipmap(device.clone());

        fut.then_signal_fence_and_flush()
            .unwrap()
            .wait(None)
            .expect("Failed to load texture");

        let texture_id = textures.insert((ImageView::new(texture)?, sampler));
        Ok(Lenna {
            texture_id,
            size: [width as f32, height as f32],
        })
    }

    fn show(&self, ui: &Ui) {
        Image::new(self.texture_id, self.size).build(ui);
    }
}

fn main() {
    let mut my_app = CustomTexturesApp::default();

    let mut system = support::init(file!());
    my_app
        .register_textures(
            system.device.clone(),
            system.queue.clone(),
            system.renderer.textures(),
        )
        .expect("Failed to register textures");
    system.main_loop(move |_, ui| my_app.show_textures(ui));
}
