use crate::{
    component::{
        block::class::ClassBlockComponent,
        block_class::model::ModelBlockClassComponent,
        block_model::{
            builder::{
                BuilderBlockModelComponent,
                CullFlags,
            },
            culling::{
                Culling,
                CullingBlockModelComponent,
            },
        },
        chunk::render_data::RenderDataChunkComponent,
    },
    resource::render_pool::primitives::Vertex,
};
use arrayvec::ArrayVec;
use rayon::prelude::*;
use voxbrix_common::{
    component::block::{
        sky_light::{
            SkyLight,
            SkyLightBlockComponent,
        },
        BlocksVec,
    },
    entity::{
        block::Neighbor,
        block_class::BlockClass,
        chunk::Chunk,
    },
    math::Vec3I32,
};
use voxbrix_world::{
    System,
    SystemData,
};

fn neighbors_to_cull_flags(
    neighbors: &[Neighbor; 6],
    this_chunk: &BlocksVec<BlockClass>,
    neighbor_chunks: &[Option<&BlocksVec<BlockClass>>; 6],
    model_bcc: &ModelBlockClassComponent,
    culling_bmc: &CullingBlockModelComponent,
) -> CullFlags {
    let mut cull_flags = CullFlags::all();
    for (i, (neighbor, neighbor_chunk)) in neighbors.iter().zip(neighbor_chunks.iter()).enumerate()
    {
        let side = CullFlags::from_index(i);

        match neighbor {
            Neighbor::ThisChunk(n) => {
                let class = this_chunk.get(*n);
                let culling = model_bcc
                    .get(class)
                    .and_then(|model| culling_bmc.get(model));
                match culling {
                    Some(Culling::Full) => {
                        cull_flags.remove(side);
                    },
                    None => {},
                }
            },
            Neighbor::OtherChunk(n) => {
                if let Some(chunk) = neighbor_chunk {
                    let class = chunk.get(*n);
                    let culling = model_bcc
                        .get(class)
                        .and_then(|model| culling_bmc.get(model));
                    match culling {
                        Some(Culling::Full) => {
                            cull_flags.remove(side);
                        },
                        None => {},
                    }
                } else {
                    cull_flags.remove(side);
                }
            },
        }
    }

    cull_flags
}

fn build_chunk_buffer_shard<'a>(
    chunk: &'a Chunk,
    class_bc: &'a ClassBlockComponent,
    model_bcc: &'a ModelBlockClassComponent,
    builder_bmc: &'a BuilderBlockModelComponent,
    culling_bmc: &'a CullingBlockModelComponent,
    sky_light_bc: &'a SkyLightBlockComponent,
) -> impl ParallelIterator<Item = Vertex> + 'a {
    let neighbor_chunk_ids = [
        [-1, 0, 0],
        [1, 0, 0],
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
    ]
    .map(|offset| chunk.checked_add(Vec3I32::from_array(offset)));

    let this_chunk_class = class_bc.get_chunk(chunk).unwrap();
    let this_chunk_light = sky_light_bc.get_chunk(chunk).unwrap();

    let neighbor_chunk_class = neighbor_chunk_ids.map(|chunk| {
        let block_classes = class_bc.get_chunk(&chunk?)?;

        Some(block_classes)
    });

    let neighbor_chunk_light = neighbor_chunk_ids.map(|chunk| {
        let block_light = sky_light_bc.get_chunk(&chunk?)?;

        Some(block_light)
    });

    this_chunk_class
        .par_iter()
        .flat_map_iter(move |(block, block_class)| {
            model_bcc
                .get(block_class)
                .and_then(|m| builder_bmc.get(m))
                .into_iter()
                .flat_map(move |model_builder| {
                    let neighbors = block.neighbors();

                    let cull_flags = neighbors_to_cull_flags(
                        &neighbors,
                        this_chunk_class,
                        &neighbor_chunk_class,
                        model_bcc,
                        culling_bmc,
                    );

                    let sky_light_levels = neighbors
                        .iter()
                        .zip(neighbor_chunk_light)
                        .map(|(neighbor, neighbor_chunk_light)| {
                            Some(match neighbor {
                                Neighbor::ThisChunk(block) => *this_chunk_light.get(*block),
                                Neighbor::OtherChunk(block) => *neighbor_chunk_light?.get(*block),
                            })
                        })
                        .map(|light| light.unwrap_or(SkyLight::MIN))
                        .collect::<ArrayVec<_, 6>>()
                        .into_inner()
                        .unwrap_or_else(|_| unreachable!());

                    model_builder.build(chunk, block, cull_flags, sky_light_levels)
                })
        })
}

pub struct BlockModelSystem;

impl System for BlockModelSystem {
    type Data<'a> = BlockModelSystemData<'a>;
}

#[derive(SystemData)]
pub struct BlockModelSystemData<'a> {
    class_bc: &'a ClassBlockComponent,
    model_bcc: &'a ModelBlockClassComponent,
    builder_bmc: &'a BuilderBlockModelComponent,
    culling_bmc: &'a CullingBlockModelComponent,
    sky_light_bc: &'a SkyLightBlockComponent,
    render_data_cc: &'a mut RenderDataChunkComponent,
}

impl BlockModelSystemData<'_> {
    pub fn run(self) {
        let chunk_exists = |chunk: &Chunk| -> bool {
            self.class_bc.get_chunk(chunk).is_some() && self.sky_light_bc.get_chunk(chunk).is_some()
        };

        let selected_chunks = self.render_data_cc.select_chunks(chunk_exists);

        let par_iter = selected_chunks.into_par_iter().map(|(chunk, mut shard)| {
            shard.par_extend(build_chunk_buffer_shard(
                &chunk,
                self.class_bc,
                self.model_bcc,
                self.builder_bmc,
                self.culling_bmc,
                self.sky_light_bc,
            ));

            (chunk, shard)
        });

        self.render_data_cc.submit_vertices(par_iter);
    }
}
