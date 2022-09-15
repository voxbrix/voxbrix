use bytemuck::{
    Pod,
    Zeroable,
};
use std::mem;
use wgpu::*;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub texture_index: u32,
    pub texture_position: [f32; 2],
}

impl Vertex {
    pub fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &[
                VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: mem::size_of::<[f32; 3]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Uint32,
                },
                VertexAttribute {
                    offset: (mem::size_of::<[f32; 3]>() + mem::size_of::<u32>()) as BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Float32x2,
                },
            ],
        }
    }
}
