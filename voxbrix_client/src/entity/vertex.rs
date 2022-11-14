use bytemuck::{
    Pod,
    Zeroable,
};
use std::mem;
use wgpu::*;

#[derive(Copy, Clone, Debug)]
pub struct Index(u32);

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub chunk: [i32; 3],
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
                    format: VertexFormat::Sint32x3,
                },
                VertexAttribute {
                    offset: mem::size_of::<[i32; 3]>() as BufferAddress,
                    shader_location: 1,
                    format: VertexFormat::Float32x3,
                },
                VertexAttribute {
                    offset: (mem::size_of::<[i32; 3]>() + mem::size_of::<[f32; 3]>())
                        as BufferAddress,
                    shader_location: 2,
                    format: VertexFormat::Uint32,
                },
                VertexAttribute {
                    offset: (mem::size_of::<[i32; 3]>()
                        + mem::size_of::<[f32; 3]>()
                        + mem::size_of::<u32>()) as BufferAddress,
                    shader_location: 3,
                    format: VertexFormat::Float32x2,
                },
            ],
        }
    }
}
