use crate::{
    component::block_model::BlockModelComponent,
    system::render::primitives::{
        Polygon,
        Vertex,
    },
};
use anyhow::Error;
use bitflags::bitflags;
use serde::Deserialize;
use voxbrix_common::{
    entity::chunk::Chunk,
    entity::block::BlockCoords,
    math::Vec3I32,
    ArrayExt,
    LabelMap,
};

pub type BuilderBlockModelComponent = BlockModelComponent<BlockModelBuilder>;

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(tag = "type")]
enum CullingNeighbor {
    None,
    NegativeX,
    PositiveX,
    NegativeY,
    PositiveY,
    NegativeZ,
    PositiveZ,
}

#[derive(Deserialize, Debug)]
struct BlockModelDescriptorVertex {
    position: BlockCoords,
    texture_position: [usize; 2],
}

#[derive(Deserialize, Debug)]
struct BlockModelDescriptorPolygon {
    texture_label: String,
    culling_neighbor: CullingNeighbor,
    vertices: [BlockModelDescriptorVertex; 4],
}

pub struct BlockModelContext<'a> {
    pub block_texture_label_map: &'a LabelMap<u32>,
}

#[derive(Deserialize, Debug)]
pub struct BlockModelBuilderDescriptor {
    grid_size: BlockCoords,
    texture_grid_size: [usize; 2],
    polygons: Vec<BlockModelDescriptorPolygon>,
}

impl BlockModelBuilderDescriptor {
    pub fn describe(&self, context: &BlockModelContext) -> Result<BlockModelBuilder, Error> {
        Ok(BlockModelBuilder {
            polygons: self
                .polygons
                .iter()
                .map(|desc| {
                    Ok::<_, Error>(PolygonBuilder {
                        culling_neighbor: desc.culling_neighbor,
                        texture_index: context
                            .block_texture_label_map
                            .get(&desc.texture_label)
                            .ok_or_else(|| {
                                Error::msg(format!(
                                    "block texture label \"{}\" is undefined",
                                    &desc.texture_label
                                ))
                            })?,
                        vertices: desc.vertices.map_ref(
                            |BlockModelDescriptorVertex {
                                 position,
                                 texture_position,
                             }| {
                                VertexBuilder {
                                    position: [
                                        position[0] as f32 / self.grid_size[0] as f32,
                                        position[1] as f32 / self.grid_size[1] as f32,
                                        position[2] as f32 / self.grid_size[2] as f32,
                                    ],
                                    texture_position: [
                                        texture_position[0] as f32
                                            / self.texture_grid_size[0] as f32,
                                        texture_position[1] as f32
                                            / self.texture_grid_size[1] as f32,
                                    ],
                                }
                            },
                        ),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?,
        })
    }
}

bitflags! {
    #[derive(Clone, Copy)]
    pub struct CullFlags: u8 {
        const X0 = 0b00000001;
        const X1 = 0b00000010;
        const Y0 = 0b00000100;
        const Y1 = 0b00001000;
        const Z0 = 0b00010000;
        const Z1 = 0b00100000;
    }
}

impl CullFlags {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::X0,
            1 => Self::X1,
            2 => Self::Y0,
            3 => Self::Y1,
            4 => Self::Z0,
            5 => Self::Z1,
            _ => panic!("incorrect side index"),
        }
    }
}

struct VertexBuilder {
    position: [f32; 3],
    texture_position: [f32; 2],
}

struct PolygonBuilder {
    culling_neighbor: CullingNeighbor,
    texture_index: u32,
    vertices: [VertexBuilder; 4],
}

pub struct BlockModelBuilder {
    polygons: Vec<PolygonBuilder>,
}

impl BlockModelBuilder {
    pub fn build(
        &self,
        polygons: &mut Vec<Polygon>,
        chunk: &Chunk,
        block: BlockCoords,
        cull_mask: CullFlags,
        sky_light_level: [u8; 6],
    ) {
        let extension = self
            .polygons
            .iter()
            .filter(|pb| {
                match pb.culling_neighbor {
                    CullingNeighbor::None => true,
                    CullingNeighbor::NegativeX => cull_mask.contains(CullFlags::X0),
                    CullingNeighbor::PositiveX => cull_mask.contains(CullFlags::X1),
                    CullingNeighbor::NegativeY => cull_mask.contains(CullFlags::Y0),
                    CullingNeighbor::PositiveY => cull_mask.contains(CullFlags::Y1),
                    CullingNeighbor::NegativeZ => cull_mask.contains(CullFlags::Z0),
                    CullingNeighbor::PositiveZ => cull_mask.contains(CullFlags::Z1),
                }
            })
            .map(|pb| {
                Polygon {
                    chunk: chunk.position.to_array(),
                    texture_index: pb.texture_index,
                    vertices: pb.vertices.map_ref(|vxb| {
                        let mut position = vxb.position;

                        position[0] += block[0] as f32;
                        position[1] += block[1] as f32;
                        position[2] += block[2] as f32;

                        let sky_light_level = match pb.culling_neighbor {
                            // TODO better lighting for non-cullable polygons
                            CullingNeighbor::None => {
                                let light_float = sky_light_level
                                    .iter()
                                    .map(|side_light| *side_light as f32)
                                    .sum::<f32>()
                                    / 6.0;

                                light_float.min(u8::MAX as f32) as u8
                            },
                            CullingNeighbor::NegativeX => sky_light_level[0],
                            CullingNeighbor::PositiveX => sky_light_level[1],
                            CullingNeighbor::NegativeY => sky_light_level[2],
                            CullingNeighbor::PositiveY => sky_light_level[3],
                            CullingNeighbor::NegativeZ => sky_light_level[4],
                            CullingNeighbor::PositiveZ => sky_light_level[5],
                        };

                        Vertex {
                            position,
                            texture_position: vxb.texture_position,
                            light_level: [sky_light_level, 0, 0, 0],
                        }
                    }),
                }
            });

        polygons.extend(extension);
    }
}

pub fn side_highlighting(chunk: Vec3I32, block_coords: BlockCoords, side: usize) -> Polygon {
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

    Polygon {
        chunk: chunk.into(),
        texture_index: TEXTURE_INDEX,
        vertices: [
            Vertex {
                position: positions[0],
                texture_position: [0.0, 0.0],
                light_level: [16, 0, 0, 0],
            },
            Vertex {
                position: positions[1],
                texture_position: [1.0, 0.0],
                light_level: [16, 0, 0, 0],
            },
            Vertex {
                position: positions[2],
                texture_position: [1.0, 1.0],
                light_level: [16, 0, 0, 0],
            },
            Vertex {
                position: positions[3],
                texture_position: [0.0, 1.0],
                light_level: [16, 0, 0, 0],
            },
        ],
    }
}
