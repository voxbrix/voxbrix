use crate::{
    component::block_model::BlockModelComponent,
    entity::texture::Texture,
    resource::render_pool::primitives::Vertex,
};
use anyhow::Error;
use bitflags::bitflags;
use serde::Deserialize;
use voxbrix_common::{
    component::block::sky_light::SkyLight,
    entity::{
        block::Block,
        chunk::Chunk,
    },
    ArrayExt,
    LabelLibrary,
};

pub type BuilderBlockModelComponent = BlockModelComponent<BlockModelBuilder>;
const VERTEX_TEXTURE_POSITION_OFFSET: f32 = 0.00001;

#[derive(Deserialize, Debug, Clone, Copy)]
#[serde(tag = "kind")]
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
struct BlockModelDescriptorQuad {
    texture_label: String,
    culling_neighbor: CullingNeighbor,
    vertices: [BlockModelDescriptorVertex; 4],
}

#[derive(Deserialize, Debug)]
pub struct BlockModelBuilderDescriptor {
    grid_size: [usize; 3],
    texture_grid_size: [usize; 2],
    quads: Vec<BlockModelDescriptorQuad>,
}

impl BlockModelBuilderDescriptor {
    pub fn describe(&self, label_library: &LabelLibrary) -> Result<BlockModelBuilder, Error> {
        Ok(BlockModelBuilder {
            quads: self
                .quads
                .iter()
                .map(|desc| {
                    let texture: Texture =
                        label_library.get(&desc.texture_label).ok_or_else(|| {
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

                            [0, 1].map(|i| coords[i] + sum[i])
                        });

                    let side_texture_center = texture_coords_sum.map(|sum| sum / 4.0);

                    Ok::<_, Error>(QuadBuilder {
                        culling_neighbor: desc.culling_neighbor,
                        texture_index: texture.as_u32(),
                        vertices: desc.vertices.map_ref(
                            |BlockModelDescriptorVertex {
                                 position,
                                 texture_position,
                             }| {
                                let texture_position = [0, 1].map(|i| {
                                    (texture_position[i] as f32) / self.texture_grid_size[i] as f32
                                });

                                let texture_position = [0, 1].map(|i| {
                                    let correction_sign =
                                        side_texture_center[i] - texture_position[i];

                                    texture_position[i]
                                        + VERTEX_TEXTURE_POSITION_OFFSET.copysign(correction_sign)
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

struct QuadBuilder {
    culling_neighbor: CullingNeighbor,
    texture_index: u32,
    vertices: [VertexBuilder; 4],
}

pub struct BlockModelBuilder {
    quads: Vec<QuadBuilder>,
}

impl BlockModelBuilder {
    pub fn build<'a>(
        &'a self,
        chunk: &'a Chunk,
        block: Block,
        cull_mask: CullFlags,
        sky_light_level: [SkyLight; 6],
    ) -> impl Iterator<Item = Vertex> + 'a {
        let block = block.into_coords().map(|bi| bi as f32);

        self.quads
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
            .flat_map(move |pb| {
                pb.vertices.map_ref(|vxb| {
                    Vertex {
                        chunk: chunk.position,
                        texture_index: pb.texture_index,
                        offset: [0, 1, 2].map(|i| vxb.position[i] + block[i]),
                        texture_position: vxb.texture_position,
                        light_parameters: {
                            let sky_light_level = match pb.culling_neighbor {
                                // TODO better lighting for non-cullable quads
                                CullingNeighbor::None => {
                                    let light_float = sky_light_level
                                        .iter()
                                        .map(|side_light| side_light.value() as f32)
                                        .sum::<f32>()
                                        / 6.0;

                                    SkyLight::from_value(light_float as u8)
                                },
                                CullingNeighbor::NegativeX => sky_light_level[0],
                                CullingNeighbor::PositiveX => sky_light_level[1],
                                CullingNeighbor::NegativeY => sky_light_level[2],
                                CullingNeighbor::PositiveY => sky_light_level[3],
                                CullingNeighbor::NegativeZ => sky_light_level[4],
                                CullingNeighbor::PositiveZ => sky_light_level[5],
                            };

                            let mut light = 0;

                            light = (light & !0xFF) | (sky_light_level.value() as u32);

                            light
                        },
                    }
                })
            })
    }
}
