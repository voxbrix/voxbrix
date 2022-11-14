use crate::{
    component::block_class::BlockClassComponent,
    entity::{
        chunk::Chunk,
        vertex::Vertex,
    },
};
use bitmask::bitmask;

pub type ModelBlockClassComponent = BlockClassComponent<Model>;

pub enum Model {
    Cube(Cube),
}

impl Model {
    pub fn to_vertices(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        chunk: &Chunk,
        block: [u8; 3],
        cull_mask: CullMask,
    ) {
        match self {
            Self::Cube(cube) => cube.to_vertices(chunk, vertices, indices, block, cull_mask),
        }
    }
}

pub struct Cube {
    pub textures: [u32; 6],
}

bitmask! {
    pub mask CullMask: u8 where flags CullMaskSides {
        X0 = 0b00000001,
        X1 = 0b00000010,
        Y0 = 0b00000100,
        Y1 = 0b00001000,
        Z0 = 0b00010000,
        Z1 = 0b00100000,
    }
}

impl CullMaskSides {
    pub fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::X0),
            1 => Some(Self::X1),
            2 => Some(Self::Y0),
            3 => Some(Self::Y1),
            4 => Some(Self::Z0),
            5 => Some(Self::Z1),
            _ => None,
        }
    }
}

impl Cube {
    fn add_side(
        chunk: [i32; 3],
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        positions: [[i32; 3]; 4],
        texture_index: u32,
    ) {
        let base = vertices.len() as u32;

        vertices.push(Vertex {
            chunk,
            position: positions[0].map(|c| c as f32),
            texture_index,
            texture_position: [0.0, 0.0],
        });

        vertices.push(Vertex {
            chunk,
            position: positions[1].map(|c| c as f32),
            texture_index,
            texture_position: [1.0, 0.0],
        });

        vertices.push(Vertex {
            chunk,
            position: positions[2].map(|c| c as f32),
            texture_index,
            texture_position: [1.0, 1.0],
        });

        vertices.push(Vertex {
            chunk,
            position: positions[3].map(|c| c as f32),
            texture_index,
            texture_position: [0.0, 1.0],
        });

        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 3);

        indices.push(base + 2);
        indices.push(base + 3);
        indices.push(base + 1);
    }

    pub fn to_vertices(
        &self,
        chunk: &Chunk,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        block: [u8; 3],
        cull_mask: CullMask,
    ) {
        let x = block[0] as i32;
        let y = block[1] as i32;
        let z = block[2] as i32;

        if cull_mask.contains(CullMaskSides::X0) {
            // Back
            Self::add_side(
                chunk.position,
                vertices,
                indices,
                [[x, y, z + 1], [x, y + 1, z + 1], [x, y + 1, z], [x, y, z]],
                self.textures[0],
            );
        }

        if cull_mask.contains(CullMaskSides::X1) {
            // Forward
            Self::add_side(
                chunk.position,
                vertices,
                indices,
                [
                    [x + 1, y + 1, z + 1],
                    [x + 1, y, z + 1],
                    [x + 1, y, z],
                    [x + 1, y + 1, z],
                ],
                self.textures[1],
            );
        }

        if cull_mask.contains(CullMaskSides::Y0) {
            // Left
            Self::add_side(
                chunk.position,
                vertices,
                indices,
                [[x + 1, y, z + 1], [x, y, z + 1], [x, y, z], [x + 1, y, z]],
                self.textures[2],
            );
        }

        if cull_mask.contains(CullMaskSides::Y1) {
            // Right
            Self::add_side(
                chunk.position,
                vertices,
                indices,
                [
                    [x, y + 1, z + 1],
                    [x + 1, y + 1, z + 1],
                    [x + 1, y + 1, z],
                    [x, y + 1, z],
                ],
                self.textures[3],
            );
        }

        if cull_mask.contains(CullMaskSides::Z0) {
            // Down
            Self::add_side(
                chunk.position,
                vertices,
                indices,
                [[x, y, z], [x, y + 1, z], [x + 1, y + 1, z], [x + 1, y, z]],
                self.textures[4],
            );
        }

        if cull_mask.contains(CullMaskSides::Z1) {
            // Up
            Self::add_side(
                chunk.position,
                vertices,
                indices,
                [
                    [x + 1, y, z + 1],
                    [x + 1, y + 1, z + 1],
                    [x, y + 1, z + 1],
                    [x, y, z + 1],
                ],
                self.textures[5],
            );
        }
    }
}
