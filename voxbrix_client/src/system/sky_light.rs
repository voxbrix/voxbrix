use crate::component::{
    block::class::ClassBlockComponent,
    chunk::{
        render_data::{
            BlkRenderDataChunkComponent,
            EnvRenderDataChunkComponent,
        },
        sky_light_data::{
            BlockQueue,
            SkyLightDataChunkComponent,
        },
    },
};
use arrayvec::ArrayVec;
use rayon::prelude::*;
use voxbrix_common::{
    component::{
        block::{
            sky_light::{
                SkyLight,
                SkyLightBlockComponent,
            },
            BlocksVec,
        },
        block_class::opacity::{
            Opacity,
            OpacityBlockClassComponent,
        },
    },
    entity::{
        block::Neighbor,
        chunk::Chunk,
    },
    math::Vec3I32,
};
use voxbrix_world::{
    System,
    SystemData,
};

const SKY_SIDE: usize = 5;
const GROUND_SIDE: usize = 4;

pub struct SkyLightSystem {
    buffer: Vec<(
        Chunk,
        Option<BlocksVec<SkyLight>>,
        Option<BlockQueue>,
        [bool; 6],
    )>,
}

impl SkyLightSystem {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }
}

impl System for SkyLightSystem {
    type Data<'a> = SkyLightSystemData<'a>;
}

#[derive(SystemData)]
pub struct SkyLightSystemData<'a> {
    system: &'a mut SkyLightSystem,
    class_bc: &'a ClassBlockComponent,
    opacity_bcc: &'a OpacityBlockClassComponent,
    sky_light_bc: &'a mut SkyLightBlockComponent,
    sky_light_data_cc: &'a mut SkyLightDataChunkComponent,
    blk_render_data_cc: &'a mut BlkRenderDataChunkComponent,
    env_render_data_cc: &'a mut EnvRenderDataChunkComponent,
}

impl<'a> SkyLightSystemData<'a> {
    pub fn run(self, number_of_blocks: usize) {
        self.system.buffer.extend(
            self.sky_light_data_cc
                .drain_chunk_queue()
                .map(|(chunk, block_queue)| {
                    let sky_light = self.sky_light_bc.remove_chunk(&chunk);

                    (chunk, sky_light, block_queue, [false; 6])
                })
                .take(rayon::current_num_threads()),
        );

        self.system.buffer.par_iter_mut().for_each(
            |(chunk, sky_light, block_queue, neighbors_need_redraw)| {
                let is_new_chunk = sky_light.is_none();

                if sky_light.is_none() {
                    *sky_light = Some(BlocksVec::new_cloned(SkyLight::MIN));
                }

                if block_queue.is_none() {
                    *block_queue = Some(BlockQueue::new_full(is_new_chunk));
                }

                let sky_light = sky_light.as_mut().unwrap();
                let block_queue = block_queue.as_mut().unwrap();

                let neighbor_chunk_ids = [
                    [-1, 0, 0],
                    [1, 0, 0],
                    [0, -1, 0],
                    [0, 1, 0],
                    [0, 0, -1],
                    [0, 0, 1],
                ]
                .map(|offset| chunk.checked_add(Vec3I32::from_array(offset)));

                let neighbor_chunks = neighbor_chunk_ids
                    .into_iter()
                    .map(|chunk| {
                        let chunk = chunk?;
                        let block_light = self.sky_light_bc.get_chunk(&chunk)?;

                        Some((chunk, block_light))
                    })
                    .collect::<ArrayVec<_, 6>>()
                    .into_inner()
                    .unwrap_or_else(|_| unreachable!());

                let classes = self
                    .class_bc
                    .get_chunk(&chunk)
                    .expect("undefined block classes for chunk");

                let mut block_counter = 0;

                loop {
                    if block_counter >= number_of_blocks {
                        break;
                    }

                    block_counter += 1;

                    let Some(block) = block_queue.pop() else {
                        break;
                    };

                    let prev_light = *sky_light.get(block);

                    let class = classes.get(block);

                    let light = match self.opacity_bcc.get(class) {
                        Some(Opacity::Full) => SkyLight::MIN,
                        None => {
                            let mut light = SkyLight::MIN;

                            for (side, neighbor) in block.neighbors().into_iter().enumerate() {
                                let neighbor_light = match neighbor {
                                    Neighbor::ThisChunk(block) => *sky_light.get(block),
                                    Neighbor::OtherChunk(block) => {
                                        match neighbor_chunks[side] {
                                            Some((_, block_light)) => *block_light.get(block),
                                            None => {
                                                if side == GROUND_SIDE {
                                                    SkyLight::MIN
                                                } else {
                                                    SkyLight::MAX
                                                }
                                            },
                                        }
                                    },
                                };

                                let new_light =
                                    if side == SKY_SIDE && neighbor_light == SkyLight::MAX {
                                        SkyLight::MAX
                                    } else {
                                        neighbor_light.fade()
                                    };

                                light = light.max(new_light);
                            }

                            light
                        },
                    };

                    *sky_light.get_mut(block) = light;

                    if light == prev_light {
                        continue;
                    }

                    for (side, neighbor) in block.neighbors().into_iter().enumerate() {
                        let neighbor_light = match neighbor {
                            Neighbor::ThisChunk(block) => *sky_light.get(block),
                            Neighbor::OtherChunk(block) => {
                                neighbors_need_redraw[side] = true;

                                match neighbor_chunks[side] {
                                    Some((_, block_light)) => *block_light.get(block),
                                    None => {
                                        if side == GROUND_SIDE {
                                            SkyLight::MIN
                                        } else {
                                            SkyLight::MAX
                                        }
                                    },
                                }
                            },
                        };

                        // For light increase:
                        // Must be added to the block_queue, as current block maybe providing
                        // light to them now.
                        // For light decrease:
                        // Previously we might have provided light to the neighbor
                        // Add it to the queue to recalculate.

                        if light > prev_light && light > neighbor_light
                            || light < prev_light
                                && light <= neighbor_light
                                && (prev_light > neighbor_light
                                    || side == GROUND_SIDE && neighbor_light == SkyLight::MAX)
                        {
                            match neighbor {
                                Neighbor::ThisChunk(block) => block_queue.push_this_chunk(block),
                                Neighbor::OtherChunk(block) => {
                                    block_queue.push_other_chunk(side, block);
                                },
                            }
                        }
                    }
                }
            },
        );

        for (chunk, sky_light, block_queue, neighbors_need_redraw) in self.system.buffer.drain(..) {
            let sky_light = sky_light.unwrap();
            let mut block_queue = block_queue.unwrap();

            // Fine to do it before inserting everything from the batch into the components and
            // queues, all neighbors are not in this batch
            let neighbor_chunks = [
                [-1, 0, 0],
                [1, 0, 0],
                [0, -1, 0],
                [0, 1, 0],
                [0, 0, -1],
                [0, 0, 1],
            ]
            .into_iter()
            .enumerate()
            .filter_map(|(side, offset)| {
                let chunk = chunk.checked_add(Vec3I32::from_array(offset))?;

                Some((side, chunk))
            });

            for (side, chunk) in neighbor_chunks {
                let Some(queue) = self.sky_light_data_cc.get_block_queue_mut(&chunk) else {
                    continue;
                };

                let mut has_new = false;

                for block in block_queue.drain_other_chunk_on_side(side) {
                    queue.push_this_chunk(block);
                    has_new = true;
                }

                if has_new {
                    self.sky_light_data_cc.enqueue_chunk(chunk);
                }
            }

            if !block_queue.is_empty() {
                self.sky_light_data_cc.enqueue_chunk(chunk);
            }

            self.sky_light_bc.insert_chunk(chunk, sky_light);
            self.sky_light_data_cc
                .insert_block_queue(chunk, block_queue);

            self.blk_render_data_cc.enqueue_chunk(chunk);
            self.env_render_data_cc.enqueue_chunk(chunk);

            let need_redraw_iter = [
                [-1, 0, 0],
                [1, 0, 0],
                [0, -1, 0],
                [0, 1, 0],
                [0, 0, -1],
                [0, 0, 1],
            ]
            .map(|offset| chunk.checked_add(Vec3I32::from_array(offset)))
            .into_iter()
            .zip(neighbors_need_redraw)
            .filter_map(|(chunk, needs_redraw)| {
                if !needs_redraw {
                    return None;
                }
                chunk
            });

            for chunk in need_redraw_iter {
                self.blk_render_data_cc.enqueue_chunk(chunk);
                self.env_render_data_cc.enqueue_chunk(chunk);
            }
        }
    }
}
