use crate::system::render::vertex::Vertex;
use bitmask::bitmask;
use serde::Deserialize;
use voxbrix_common::{
    component::block_class::BlockClassComponent,
    entity::chunk::Chunk,
    math::Vec3,
};

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
        block: [usize; 3],
        cull_mask: CullMask,
        sky_light_level: [u8; 6],
    ) {
        match self {
            Self::Cube(cube) => {
                cube.to_vertices(chunk, vertices, indices, block, cull_mask, sky_light_level)
            },
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
    pub fn add_side_highlighting(
        chunk: Vec3<i32>,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        block_coords: [usize; 3],
        side: usize,
    ) {
        const ELEVATION: f32 = 0.01;
        const TEXTURE_INDEX: u32 = 0;

        let [x, y, z] = block_coords;

        let positions = match side {
            0 => [[x, y, z + 1], [x, y + 1, z + 1], [x, y + 1, z], [x, y, z]],
            1 => {
                [
                    [x + 1, y + 1, z + 1],
                    [x + 1, y, z + 1],
                    [x + 1, y, z],
                    [x + 1, y + 1, z],
                ]
            },
            2 => [[x + 1, y, z + 1], [x, y, z + 1], [x, y, z], [x + 1, y, z]],
            3 => {
                [
                    [x, y + 1, z + 1],
                    [x + 1, y + 1, z + 1],
                    [x + 1, y + 1, z],
                    [x, y + 1, z],
                ]
            },
            4 => [[x, y, z], [x, y + 1, z], [x + 1, y + 1, z], [x + 1, y, z]],
            5 => {
                [
                    [x + 1, y, z + 1],
                    [x + 1, y + 1, z + 1],
                    [x, y + 1, z + 1],
                    [x, y, z + 1],
                ]
            },
            _ => panic!("build_target_hightlight: incorrect side index"),
        };

        let base = vertices.len() as u32;

        let (change_axis, change_amount) = match side {
            0 => (0, -ELEVATION),
            1 => (0, ELEVATION),
            2 => (1, -ELEVATION),
            3 => (1, ELEVATION),
            4 => (2, -ELEVATION),
            5 => (2, ELEVATION),
            _ => unreachable!(),
        };

        let positions = positions.map(|a| {
            let mut result = a.map(|i| i as f32);
            result[change_axis] += change_amount;
            result
        });

        vertices.push(Vertex {
            chunk: chunk.into(),
            position: positions[0],
            texture_index: TEXTURE_INDEX,
            texture_position: [0.0, 0.0],
            light_level: [16, 0, 0, 0],
        });

        vertices.push(Vertex {
            chunk: chunk.into(),
            position: positions[1],
            texture_index: TEXTURE_INDEX,
            texture_position: [1.0, 0.0],
            light_level: [16, 0, 0, 0],
        });

        vertices.push(Vertex {
            chunk: chunk.into(),
            position: positions[2],
            texture_index: TEXTURE_INDEX,
            texture_position: [1.0, 1.0],
            light_level: [16, 0, 0, 0],
        });

        vertices.push(Vertex {
            chunk: chunk.into(),
            position: positions[3],
            texture_index: TEXTURE_INDEX,
            texture_position: [0.0, 1.0],
            light_level: [16, 0, 0, 0],
        });

        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 3);

        indices.push(base + 2);
        indices.push(base + 3);
        indices.push(base + 1);
    }

    fn add_side(
        chunk: Vec3<i32>,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        positions: [[usize; 3]; 4],
        texture_index: u32,
        sky_light_level: u8,
    ) {
        let base = vertices.len() as u32;

        let light_level = [sky_light_level, 0, 0, 0];

        vertices.push(Vertex {
            chunk: chunk.into(),
            position: positions[0].map(|c| c as f32),
            texture_index,
            texture_position: [0.0, 0.0],
            light_level,
        });

        vertices.push(Vertex {
            chunk: chunk.into(),
            position: positions[1].map(|c| c as f32),
            texture_index,
            texture_position: [1.0, 0.0],
            light_level,
        });

        vertices.push(Vertex {
            chunk: chunk.into(),
            position: positions[2].map(|c| c as f32),
            texture_index,
            texture_position: [1.0, 1.0],
            light_level,
        });

        vertices.push(Vertex {
            chunk: chunk.into(),
            position: positions[3].map(|c| c as f32),
            texture_index,
            texture_position: [0.0, 1.0],
            light_level,
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
        block: [usize; 3],
        cull_mask: CullMask,
        sky_light_level: [u8; 6],
    ) {
        let x = block[0];
        let y = block[1];
        let z = block[2];

        if cull_mask.contains(CullMaskSides::X0) {
            // Back
            Self::add_side(
                chunk.position,
                vertices,
                indices,
                [[x, y, z + 1], [x, y + 1, z + 1], [x, y + 1, z], [x, y, z]],
                self.textures[0],
                sky_light_level[0],
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
                sky_light_level[1],
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
                sky_light_level[2],
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
                sky_light_level[3],
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
                sky_light_level[4],
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
                sky_light_level[5],
            );
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ModelDescriptor {
    Cube { textures: [String; 6] },
}
