use crate::{
    component::{
        block_model::BlockModelComponent,
        texture::location::LocationTextureComponent,
    },
    entity::texture::Texture,
    system::render::primitives::{
        Polygon,
        Vertex,
    },
};
use anyhow::Error;
use bitflags::bitflags;
use serde::Deserialize;
use voxbrix_common::{
    entity::{
        block::Block,
        chunk::Chunk,
    },
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
    position: [usize; 3],
    texture_position: [usize; 2],
}

#[derive(Deserialize, Debug)]
struct BlockModelDescriptorPolygon {
    texture_label: String,
    culling_neighbor: CullingNeighbor,
    vertices: [BlockModelDescriptorVertex; 4],
}

pub struct BlockModelContext<'a> {
    pub texture_label_map: LabelMap<Texture>,
    pub location_tc: &'a LocationTextureComponent,
}

#[derive(Deserialize, Debug)]
pub struct BlockModelBuilderDescriptor {
    grid_size: [usize; 3],
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
                    let texture = context
                        .texture_label_map
                        .get(&desc.texture_label)
                        .ok_or_else(|| {
                            Error::msg(format!(
                                "block texture label \"{}\" is undefined",
                                &desc.texture_label
                            ))
                        })?;

                    // Here is the fix for the glitchy pixels on the edge of sides
                    // (just cause those grinded my gears)
                    // Sometimes pixels on the edge of a side appear to be sampled from the outside
                    // of the designated texture area, it happens because of f32 texture position inaccuracy
                    // To compensate, find the center of the side in texture surface and move every
                    // vertex toward that center by VERTEX_TEXTURE_POSITION_OFFSET fracture
                    // of the grid size
                    // Grid size involved in correction to have approximately the same offset even for
                    // non-square textures
                    let texture_coords_sum =
                        desc.vertices.iter().fold([0.0, 0.0], |sum, vertex| {
                            let coords = [0, 1].map(|i| {
                                (vertex.texture_position[i] as f32)
                                    / (self.texture_grid_size[i] as f32)
                            });

                            let coords = context.location_tc.get_coords(texture, coords);

                            [0, 1].map(|i| coords[i] + sum[i])
                        });

                    let side_texture_center = texture_coords_sum.map(|sum| sum / 4.0);

                    Ok::<_, Error>(PolygonBuilder {
                        culling_neighbor: desc.culling_neighbor,
                        texture_index: context.location_tc.get_index(texture),
                        vertices: desc.vertices.map_ref(
                            |BlockModelDescriptorVertex {
                                 position,
                                 texture_position,
                             }| {
                                let texture_position = [0, 1].map(|i| {
                                    (texture_position[i] as f32) / self.texture_grid_size[i] as f32
                                });

                                let texture_position =
                                    context.location_tc.get_coords(texture, texture_position);

                                let correction_amplitude =
                                    context.location_tc.get_edge_correction(texture);

                                let texture_position = [0, 1].map(|i| {
                                    let correction_sign =
                                        side_texture_center[i] - texture_position[i];

                                    texture_position[i]
                                        + correction_amplitude[i].copysign(correction_sign)
                                });

                                VertexBuilder {
                                    position: [0, 1, 2]
                                        .map(|i| position[i] as f32 / self.grid_size[i] as f32),
                                    texture_position,
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
    pub fn build<'a>(
        &'a self,
        chunk: &'a Chunk,
        block: Block,
        cull_mask: CullFlags,
        sky_light_level: [u8; 6],
    ) -> impl Iterator<Item = Polygon> + 'a {
        let block = block.into_coords();

        self.polygons
            .iter()
            .filter(move |pb| {
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
            .map(move |pb| {
                Polygon {
                    chunk: chunk.position,
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
            })
    }
}
