use bytemuck::{
    Pod,
    Zeroable,
};
use std::mem;
use wgpu::*;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub chunk: [i32; 3],
    pub texture_index: u32,
    pub offset: [f32; 3],
    pub texture_position: [f32; 2],
    pub light_parameters: u32,
}

impl Vertex {
    pub fn desc<'a>() -> VertexBufferLayout<'a> {
        const VERTEX_ATTRIBUTES: &[VertexAttribute; 5] = &wgpu::vertex_attr_array![
            // chunk:
            0 => Sint32x3,
            // texture_index:
            1 => Uint32,
            // offset
            2 => Float32x3,
            // texture_position:
            3 => Float32x2,
            // light_parameters:
            4 => Uint32,
        ];

        VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: VERTEX_ATTRIBUTES,
        }
    }

    pub const fn size() -> BufferAddress {
        mem::size_of::<Self>() as BufferAddress
    }
}
