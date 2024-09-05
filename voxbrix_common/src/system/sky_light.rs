use crate::{
    component::{
        block::{
            sky_light::{
                SkyLight,
                SkyLightBlockComponent,
            },
            BlockComponent,
            Blocks,
            BlocksVec,
        },
        block_class::opacity::{
            Opacity,
            OpacityBlockClassComponent,
        },
    },
    entity::{
        block::{
            Block,
            Neighbor,
        },
        block_class::BlockClass,
        chunk::Chunk,
    },
};
use ahash::{
    AHashMap,
    AHashSet,
};
use arrayvec::ArrayVec;
use queues::{
    BlockQueue,
    ChunkQueue,
};
use rayon::prelude::*;

const SKY_SIDE: usize = 5;
const GROUND_SIDE: usize = 4;

pub struct SkyLightSystem {
    chunk_queue: ChunkQueue,
    block_queues: AHashMap<Chunk, BlockQueue>,
    // TODO: can skip all neighbors need redraw things on server
    buffer: Vec<(
        Chunk,
        Option<BlocksVec<SkyLight>>,
        Option<BlockQueue>,
        [bool; 6],
    )>,
    chunks_need_redraw: AHashSet<Chunk>,
}

impl SkyLightSystem {
    pub fn new() -> Self {
        Self {
            chunk_queue: ChunkQueue::new(),
            block_queues: AHashMap::new(),
            buffer: Vec::new(),
            chunks_need_redraw: AHashSet::new(),
        }
    }

    pub fn is_queue_empty(&self) -> bool {
        self.chunk_queue.is_empty()
    }

    pub fn enqueue_chunk(&mut self, chunk: Chunk) {
        self.chunk_queue.push(chunk);
    }

    pub fn block_change(&mut self, chunk: &Chunk, block: Block) {
        if let Some(queue) = self.block_queues.get_mut(&chunk) {
            queue.push_this_chunk(block);
            self.enqueue_chunk(*chunk);
        }
    }

    pub fn remove_chunk(&mut self, chunk: &Chunk) {
        self.chunk_queue.remove(chunk);
        self.block_queues.remove(chunk);
    }

    pub fn process<'a>(
        &'a mut self,
        number_of_blocks: usize,
        class_bc: &(impl BlockComponent<BlockClass> + Send + Sync),
        opacity_bcc: &OpacityBlockClassComponent,
        sky_light_bc: &mut SkyLightBlockComponent,
    ) -> impl ExactSizeIterator<Item = Chunk> + 'a {
        self.buffer.extend(
            self.chunk_queue
                .get_queue()
                .map(|chunk| {
                    let sky_light = sky_light_bc.remove_chunk(&chunk);
                    let block_queue = self.block_queues.remove(&chunk);

                    (chunk, sky_light, block_queue, [false; 6])
                })
                .take(rayon::current_num_threads()),
        );

        self.buffer.par_iter_mut().for_each(
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
                .map(|offset| chunk.checked_add(offset));

                let neighbor_chunks = neighbor_chunk_ids
                    .into_iter()
                    .map(|chunk| {
                        let chunk = chunk?;
                        let block_light = sky_light_bc.get_chunk(&chunk)?;

                        Some((chunk, block_light))
                    })
                    .collect::<ArrayVec<_, 6>>()
                    .into_inner()
                    .unwrap_or_else(|_| unreachable!());

                let classes = class_bc
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

                    let light = match opacity_bcc.get(class) {
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

        for (chunk, sky_light, block_queue, neighbors_need_redraw) in self.buffer.drain(..) {
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
                let chunk = chunk.checked_add(offset)?;

                Some((side, chunk))
            });

            for (side, chunk) in neighbor_chunks {
                let Some(queue) = self.block_queues.get_mut(&chunk) else {
                    continue;
                };

                let mut has_new = false;

                for block in block_queue.drain_other_chunk_on_side(side) {
                    queue.push_this_chunk(block);
                    has_new = true;
                }

                if has_new {
                    self.chunk_queue.push(chunk);
                }
            }

            if !block_queue.is_empty() {
                self.chunk_queue.push(chunk);
            }

            sky_light_bc.insert_chunk(chunk, sky_light);
            self.block_queues.insert(chunk, block_queue);

            self.chunks_need_redraw.insert(chunk);

            let need_redraw_iter = [
                [-1, 0, 0],
                [1, 0, 0],
                [0, -1, 0],
                [0, 1, 0],
                [0, 0, -1],
                [0, 0, 1],
            ]
            .map(|offset| chunk.checked_add(offset))
            .into_iter()
            .zip(neighbors_need_redraw)
            .filter_map(|(chunk, needs_redraw)| {
                if !needs_redraw {
                    return None;
                }
                chunk
            });

            self.chunks_need_redraw.extend(need_redraw_iter);
        }

        self.chunks_need_redraw.drain()
    }
}

mod queues {
    use crate::entity::{
        block::{
            Block,
            BLOCKS_IN_CHUNK,
            BLOCKS_IN_CHUNK_EDGE,
        },
        chunk::Chunk,
    };
    use ahash::AHashSet;
    use std::{
        collections::VecDeque,
        iter,
    };

    pub struct BlockQueue {
        current_position: usize,
        this_chunk: Box<[bool; BLOCKS_IN_CHUNK]>,
        other_chunks: Box<[bool; BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE * 6]>,
    }

    impl BlockQueue {
        pub fn new_full(other_chunks_fill: bool) -> Self {
            Self {
                current_position: BLOCKS_IN_CHUNK - 1,
                this_chunk: Box::new([true; BLOCKS_IN_CHUNK]),
                other_chunks: Box::new(
                    [other_chunks_fill; BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE * 6],
                ),
            }
        }

        pub fn push_this_chunk(&mut self, block: Block) {
            self.this_chunk[block.as_usize()] = true;
        }

        pub fn push_other_chunk(&mut self, side: usize, block: Block) {
            let fixed_axis = side / 2;
            let coords = block.into_coords();

            let (a0, a1) = match fixed_axis {
                0 => (1, 2),
                1 => (0, 2),
                2 => (0, 1),
                _ => unreachable!(),
            };

            let index = coords[a0]
                + coords[a1] * BLOCKS_IN_CHUNK_EDGE
                + side * BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;
            self.other_chunks[index] = true;
        }

        pub fn pop(&mut self) -> Option<Block> {
            let mut counter = 0;
            while self.this_chunk[self.current_position] == false && counter < BLOCKS_IN_CHUNK {
                // For faster calculation of new chunks, calculation must be started
                // with blocks in queue ordered in sky -> ground direction
                self.current_position = self
                    .current_position
                    .checked_sub(1)
                    .unwrap_or(BLOCKS_IN_CHUNK - 1);
                counter += 1;
            }

            let is_some = self.this_chunk[self.current_position];

            self.this_chunk[self.current_position] = false;

            is_some.then_some(Block::from_usize(self.current_position).unwrap())
        }

        pub fn drain_other_chunk_on_side<'a>(
            &'a mut self,
            side: usize,
        ) -> impl Iterator<Item = Block> + 'a {
            let fixed_axis = side / 2;

            let (a0, a1) = match fixed_axis {
                0 => (1, 2),
                1 => (0, 2),
                2 => (0, 1),
                _ => unreachable!(),
            };

            // +1 here because we need neighbor's block not this chunk's
            let fixed_axis_value = ((side + 1) % 2) * (BLOCKS_IN_CHUNK_EDGE - 1);

            (0 .. BLOCKS_IN_CHUNK_EDGE)
                .flat_map(|a1val| (0 .. BLOCKS_IN_CHUNK_EDGE).map(move |a0val| (a0val, a1val)))
                .filter_map(move |(a0val, a1val)| {
                    let index = a0val
                        + a1val * BLOCKS_IN_CHUNK_EDGE
                        + side * BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;

                    if self.other_chunks[index] {
                        self.other_chunks[index] = false;

                        let mut coords = [0; 3];

                        coords[a0] = a0val;
                        coords[a1] = a1val;
                        coords[fixed_axis] = fixed_axis_value;

                        Some(Block::from_coords(coords))
                    } else {
                        None
                    }
                })
        }

        pub fn is_empty(&self) -> bool {
            self.this_chunk.iter().find(|b| **b).is_none()
        }
    }

    pub struct ChunkQueue {
        // We need to take turns with even/odd chunks because to parallelize the process
        // we remove the chunks being processed from the sky light component.
        // With 2 queues we still have neighbor chunks guaranteed to be readable from the component because
        // the neighbor chunks for odd chunks will be even chunks and vice versa.
        even_chunk_queue: VecDeque<Chunk>,
        odd_chunk_queue: VecDeque<Chunk>,
        enqueued_chunks: AHashSet<Chunk>,
        is_even_next: bool,
    }

    impl ChunkQueue {
        pub fn new() -> Self {
            Self {
                even_chunk_queue: VecDeque::new(),
                odd_chunk_queue: VecDeque::new(),
                enqueued_chunks: AHashSet::new(),
                is_even_next: true,
            }
        }

        pub fn is_empty(&self) -> bool {
            self.enqueued_chunks.is_empty()
        }

        pub fn push(&mut self, chunk: Chunk) {
            if !self.enqueued_chunks.insert(chunk) {
                return;
            }

            if chunk.position.into_iter().map(|i| i as i64).sum::<i64>() % 2 == 0 {
                self.even_chunk_queue.push_back(chunk);
            } else {
                self.odd_chunk_queue.push_back(chunk);
            }
        }

        pub fn remove(&mut self, chunk: &Chunk) -> bool {
            self.enqueued_chunks.remove(chunk)
        }

        pub fn get_queue<'a>(&'a mut self) -> impl Iterator<Item = Chunk> + 'a {
            let queue = if self.is_even_next {
                self.is_even_next = false;
                &mut self.even_chunk_queue
            } else {
                self.is_even_next = true;
                &mut self.odd_chunk_queue
            };

            iter::from_fn(|| {
                let chunk = queue.pop_front()?;

                // Lazily ignoring already removed chunks
                if !self.enqueued_chunks.remove(&chunk) {
                    return None;
                }

                Some(chunk)
            })
        }
    }
}
