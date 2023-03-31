[![crates.io](https://img.shields.io/crates/v/imgui-vulkano-renderer)](https://crates.io/crates/imgui-vulkano-renderer)
[![API Docs](https://docs.rs/imgui-vulkano-renderer/badge.svg)](https://docs.rs/imgui-vulkano-renderer/)
![Liscense](https://img.shields.io/crates/l/imgui-vulkano-renderer)

# imgui-vulkano-renderer

A [vulkano]-based renderer for [imgui-rs].

Warning: I've only used this renderer in a few examples and a couple projects, so there are likely some issues, but it seems to work with basic ImGui usage.

Supports [imgui-rs] version `0.9` and [vulkano] version `0.32`. 

## Usage

The `Renderer` struct is designed to be a drop-in replacement for the equivalent in  `imgui-glium-renderer` and `imgui-gfx-renderer` (from the [imgui-rs] repository), modulo the API-specific context arguments (the Vulkano `Device` and `Queue` structs). 

### Setup:

```rust
use imgui_vulkano_renderer::Renderer;

let mut renderer = Renderer::init(
    &mut imgui_ctx,
    device.clone(),
    graphics_queue.clone(),
    Format::R8G8B8A8Srgb
).unwrap();
```

### Rendering:

Use the `Renderer::draw_commands` function to update buffers and 

```rust

let ui = imgui_ctx.frame();

// ... UI elements created here

let draw_data = ui.render();

let mut cmd_buf_builder = AutoCommandBufferBuilder::new(device.clone(), graphics_queue.family()).unwrap();

// add Vulkan commands to a command buffer. Here a new command buffer is used, but you can also append to an existing one.
renderer.draw_commands(&mut cmd_buf_builder, graphics_queue.clone(), target_image.clone(), draw_data).unwrap();

let cmd_buf = cmd_buf_builder.build().unwrap();

```

### Misc.

The font altas texture can be reloaded with the following:

```rust
renderer.reupload_font_texture(&mut imgui_ctx, device.clone(), queue.clone());
```

Textures used in your UI are looked up in an `imgui::Textures` struct, which can be accessed with `Renderer::textures(_mut)`.

### Examples

I rewrote a couple of examples from [imgui-rs] to show basic usage (most of them only needed setup changes to the `System` struct in [`examples/support/mod.rs`](examples/support/mod.rs)). They can be run with:

```bash
cargo run --example hello_world
cargo run --example custom_textures
```


[vulkano]: https://github.com/vulkano-rs/vulkano
[imgui-rs]: https://github.com/Gekkio/imgui-rs
