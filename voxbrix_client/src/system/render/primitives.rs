use bytemuck::{
    Pod,
    Zeroable,
};
use std::mem;
use wgpu::*;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct VertexDescription {
    pub index: u32,
}

impl VertexDescription {
    pub fn desc<'a>() -> VertexBufferLayout<'a> {
        VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                0 => Uint32,
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Quad {
    pub chunk: [i32; 3],
    pub texture_index: u32,
    pub vertices: [[f32; 3]; 4],
    pub texture_positions: [[f32; 2]; 4],
    pub light_parameters: [u32; 4],
}

impl Quad {
    pub fn desc<'a>() -> VertexBufferLayout<'a> {
        const VERTEX_ATTRIBUTES: &[VertexAttribute; 8] = &wgpu::vertex_attr_array![
            // chunk:
            1 => Sint32x3,
            // texture_index:
            2 => Uint32,
            // vertices
            3 => Float32x4,
            4 => Float32x4,
            5 => Float32x4,
            // texture_positions:
            6 => Float32x4,
            7 => Float32x4,
            // light_parameters:
            8 => Uint32x4,
        ];

        VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as BufferAddress,
            step_mode: VertexStepMode::Instance,
            attributes: VERTEX_ATTRIBUTES,
        }
    }

    pub const fn size() -> BufferAddress {
        mem::size_of::<Self>() as BufferAddress
    }
}
