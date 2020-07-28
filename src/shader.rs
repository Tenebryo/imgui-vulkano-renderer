pub mod vs {
    vulkano_shaders::shader!{
        ty: "vertex",
        path: "src/shaders/shader.vert",
    }
}

pub mod fs {
    vulkano_shaders::shader!{
        ty: "fragment",
        path: "src/shaders/shader.frag",
    }
}