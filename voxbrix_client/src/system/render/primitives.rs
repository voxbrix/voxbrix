use bytemuck::{
    Pod,
    Zeroable,
};
use std::mem;
use voxbrix_common::component::block::sky_light::SkyLight;
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
pub struct Vertex {
    pub position: [f32; 3],
    pub texture_position: [f32; 2],
    pub light_level: u32,
}

impl Vertex {
    pub fn set_sky_light(&mut self, sky_light: SkyLight) {
        // Create a mask to clear the bits for the specified index
        // let mask = !(0xFF << (index * 8));

        // Clear the bits at the specified index and set the new value
        // (encoded & mask) | ((value as u32) << (index * 8))

        self.light_level = (self.light_level & !0xFF) | (sky_light.value() as u32)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Quad {
    pub chunk: [i32; 3],
    pub texture_index: u32,
    pub vertices: [Vertex; 4],
}

impl Quad {
    pub fn desc<'a>() -> VertexBufferLayout<'a> {
        const VERTEX_ATTRIBUTES: &[VertexAttribute; 14] = &wgpu::vertex_attr_array![
            1 => Sint32x3,
            2 => Uint32,
            3 => Float32x3,
            4 => Float32x2,
            5 => Uint32,
            6 => Float32x3,
            7 => Float32x2,
            8 => Uint32,
            9 => Float32x3,
            10 => Float32x2,
            11 => Uint32,
            12 => Float32x3,
            13 => Float32x2,
            14 => Uint32,
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
