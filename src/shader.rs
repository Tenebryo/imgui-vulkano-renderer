pub mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "src/shaders/shader.vert",
        types_meta: {
            use bytemuck::{Pod, Zeroable};
            #[derive(Clone,Copy,Zeroable,Pod)]
        }
    }
}

pub mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "src/shaders/shader.frag",
    }
}
