use crate::{
    component::{
        block::{
            class::ClassBlockComponent,
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
        block::{
            Block,
            BlockCoords,
            BLOCKS_IN_CHUNK_EDGE,
            BLOCKS_IN_CHUNK_USIZE,
        },
        block_class::BlockClass,
        chunk::Chunk,
    },
};
use ahash::AHashSet;
use arrayvec::ArrayVec;
use rayon::iter::{
    ParallelDrainRange,
    ParallelExtend,
    ParallelIterator,
};
use std::{
    cmp::Ordering,
    collections::VecDeque,
    iter,
    mem,
};

const SKY_SIDE: usize = 5;
const GROUND_SIDE: usize = 4;
const SKY_GROUND_AXIS: usize = 2;

fn from_sky_to_ground_sort(chunk1: &Chunk, chunk2: &Chunk) -> Ordering {
    chunk1.position[SKY_GROUND_AXIS]
        .cmp(&chunk2.position[SKY_GROUND_AXIS])
        .reverse()
}

fn from_sky_to_ground_sort_with_player(
    chunk1: &Chunk,
    chunk2: &Chunk,
    player_chunk: &Chunk,
) -> Ordering {
    from_sky_to_ground_sort(chunk1, chunk2).then_with(|| {
        // We skip SKY_GROUND_AXIS below:
        let chunk1_player_distance: i64 = [
            player_chunk.position[0] - chunk1.position[0],
            player_chunk.position[1] - chunk1.position[1],
            // player_chunk.position[2] - chunk1.position[2],
        ]
        .map(|i| (i as i64).pow(2))
        .iter()
        .sum();

        let chunk2_player_distance: i64 = [
            player_chunk.position[0] - chunk2.position[0],
            player_chunk.position[1] - chunk2.position[1],
            // player_chunk.position[2] - chunk2.position[2],
        ]
        .map(|i| (i as i64).pow(2))
        .iter()
        .sum();

        chunk1_player_distance.cmp(&chunk2_player_distance)
    })
}

#[derive(Clone, Copy)]
enum ChunkKind {
    Even,
    Odd,
}

pub struct SkyLightSystem {
    processed_chunks: AHashSet<Chunk>,
    next_compute: ChunkKind,
    chunks_to_compute_even: AHashSet<Chunk>,
    chunks_to_compute_odd: AHashSet<Chunk>,
    queue_buffer: Vec<Chunk>,
    pre_compute_buffer: Vec<(Chunk, Option<BlocksVec<SkyLight>>)>,
    post_compute_buffer: Vec<(Chunk, BlocksVec<SkyLight>, ArrayVec<Chunk, 6>)>,
}

impl SkyLightSystem {
    pub fn new() -> Self {
        Self {
            processed_chunks: AHashSet::new(),
            next_compute: ChunkKind::Even,
            chunks_to_compute_even: AHashSet::new(),
            chunks_to_compute_odd: AHashSet::new(),
            queue_buffer: Vec::new(),
            pre_compute_buffer: Vec::new(),
            post_compute_buffer: Vec::new(),
        }
    }

    /// Should only be called on existing chunk that has `ClassBlockComponent` defined,
    /// will panic otherwise.
    /// Returns the requested chunk and neighbor chunks that require recalculation.
    /// If the old light block component for the target chunk exists, it should be removed from
    /// the SkyLightBlockComponent structure and provided as argument to the function,
    /// the returned light block component should be inserted instead.
    pub fn calc_chunk(
        &self,
        chunk: Chunk,
        old_chunk_light: Option<BlocksVec<SkyLight>>,
        class_bc: &ClassBlockComponent,
        opacity_bcc: &OpacityBlockClassComponent,
        sky_light_bc: &SkyLightBlockComponent,
    ) -> (BlocksVec<SkyLight>, ArrayVec<Chunk, 6>) {
        let mut queue = VecDeque::new();

        let chunk_class = class_bc
            .get_chunk(&chunk)
            .expect("calculating light for existing chunk");

        let mut chunk_light = BlocksVec::new(vec![SkyLight::MIN; BLOCKS_IN_CHUNK_USIZE]);

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
                let block_classes = class_bc.get_chunk(&chunk)?;
                let block_light = sky_light_bc.get_chunk(&chunk)?;

                Some((chunk, block_classes, block_light))
            })
            .collect::<ArrayVec<_, 6>>()
            .into_inner()
            .unwrap_or_else(|_| unreachable!());

        // If the new lighting in the 1-block layers stays the same don't
        // recalculate inner blocks because sky light always comes externally
        let mut recalc_inner_blocks = false;

        // Fill 1-block layers on the sides with external light
        for side in 0 .. 6 {
            AddSide {
                side,
                old_chunk_light: old_chunk_light.as_ref(),
                chunk_class,
                opacity_bcc,
                chunk_light: &mut chunk_light,
                neighbor_chunks,
                recalc_inner_blocks: &mut recalc_inner_blocks,
            }
            .run();
        }

        if !recalc_inner_blocks && old_chunk_light.is_some() {
            return (old_chunk_light.unwrap(), ArrayVec::new());
        }

        for side in 0 .. 6 {
            let (axis0, axis1, fixed_axis, fixed_axis_value) = match side {
                0 => (1, 2, 0, 0),
                1 => (1, 2, 0, BLOCKS_IN_CHUNK_EDGE - 1),
                2 => (0, 2, 1, 0),
                3 => (0, 2, 1, BLOCKS_IN_CHUNK_EDGE - 1),
                4 => (0, 1, 2, 0),
                5 => (0, 1, 2, BLOCKS_IN_CHUNK_EDGE - 1),
                i => panic!("incorrect side index: {}", i),
            };

            for a0 in 0 .. BLOCKS_IN_CHUNK_EDGE {
                for a1 in 0 .. BLOCKS_IN_CHUNK_EDGE {
                    let mut block_coords = [0; 3];

                    block_coords[axis0] = a0;
                    block_coords[axis1] = a1;
                    block_coords[fixed_axis] = fixed_axis_value;

                    let block = Block::from_coords(block_coords);

                    let block_light = chunk_light.get_mut(block);

                    if *block_light > SkyLight::MIN {
                        LightDispersion {
                            block,
                            block_coords,
                            block_light: *block_light,
                            chunk_class,
                            opacity_bcc,
                            chunk_light: &mut chunk_light,
                            queue: &mut queue,
                        }
                        .disperse();
                    }
                }
            }
        }

        while let Some((block, block_coords)) = queue.pop_front() {
            let block_light = chunk_light.get_mut(block);

            LightDispersion {
                block,
                block_coords,
                block_light: *block_light,
                chunk_class,
                opacity_bcc,
                chunk_light: &mut chunk_light,
                queue: &mut queue,
            }
            .disperse();
        }

        let chunks_to_recalc = (0 .. 6)
            .filter_map(|side| {
                CheckSide {
                    side,
                    old_chunk_light: old_chunk_light.as_ref(),
                    chunk_light: &chunk_light,
                    neighbor_chunks,
                    opacity_bcc,
                }
                .needs_recalculation()
            })
            .collect();

        (chunk_light, chunks_to_recalc)
    }

    /// Computes light for the chunk and adds changed neighbors to the queue
    /// to be recalculated.
    pub fn compute_queued(
        &mut self,
        class_bc: &ClassBlockComponent,
        opacity_bcc: &OpacityBlockClassComponent,
        sky_light_bc: &mut SkyLightBlockComponent,
        player_chunk: Option<Chunk>,
    ) {
        let mut chunks_to_compute = match self.next_compute {
            ChunkKind::Even => mem::take(&mut self.chunks_to_compute_even),
            ChunkKind::Odd => mem::take(&mut self.chunks_to_compute_odd),
        };

        self.queue_buffer.clear();
        self.queue_buffer.extend(chunks_to_compute.iter());
        if let Some(player_chunk) = player_chunk {
            self.queue_buffer.sort_unstable_by(|c1, c2| {
                from_sky_to_ground_sort_with_player(c1, c2, &player_chunk)
            });
        } else {
            self.queue_buffer
                .sort_unstable_by(|c1, c2| from_sky_to_ground_sort(c1, c2));
        }
        let mut chunks_iter = self.queue_buffer.iter();

        let mut pre_compute_buffer = mem::take(&mut self.pre_compute_buffer);
        let mut post_compute_buffer = mem::take(&mut self.post_compute_buffer);

        let expansion = iter::from_fn(|| {
            let next = *chunks_iter.next()?;
            chunks_to_compute.remove(&next);
            Some(next)
        })
        .filter(|chunk| class_bc.get_chunk(chunk).is_some())
        .map(|chunk| (chunk, sky_light_bc.remove_chunk(&chunk)))
        .take(rayon::current_num_threads());

        pre_compute_buffer.clear();
        pre_compute_buffer.extend(expansion);

        let expansion = pre_compute_buffer
            .par_drain(..)
            .map(|(chunk, old_light_component)| {
                let (light_component, chunks_to_recalc) = self.calc_chunk(
                    chunk,
                    old_light_component,
                    &class_bc,
                    &opacity_bcc,
                    &sky_light_bc,
                );

                (chunk, light_component, chunks_to_recalc)
            });

        post_compute_buffer.clear();
        post_compute_buffer.par_extend(expansion);

        let expansion =
            post_compute_buffer
                .drain(..)
                .flat_map(|(chunk, light_component, chunks_to_recalc)| {
                    sky_light_bc.insert_chunk(chunk, light_component);
                    self.processed_chunks.insert(chunk);
                    chunks_to_recalc.into_iter()
                });

        match self.next_compute {
            ChunkKind::Even => {
                self.next_compute = ChunkKind::Odd;
                self.chunks_to_compute_even = chunks_to_compute;
                self.chunks_to_compute_odd.extend(expansion);
            },
            ChunkKind::Odd => {
                self.next_compute = ChunkKind::Even;
                self.chunks_to_compute_odd = chunks_to_compute;
                self.chunks_to_compute_even.extend(expansion);
            },
        }

        self.pre_compute_buffer = pre_compute_buffer;
        self.post_compute_buffer = post_compute_buffer;
    }

    pub fn add_chunk(&mut self, chunk: Chunk) {
        if chunk.position.iter().sum::<i32>() % 2 == 0 {
            self.chunks_to_compute_even.insert(chunk);
        } else {
            self.chunks_to_compute_odd.insert(chunk);
        }
    }

    pub fn drain_processed_chunks<'a>(&'a mut self) -> impl Iterator<Item = Chunk> + 'a {
        self.processed_chunks.drain()
    }

    pub fn has_chunks_in_queue(&self) -> bool {
        !self.chunks_to_compute_even.is_empty() || !self.chunks_to_compute_odd.is_empty()
    }
}

struct LightDispersion<'a> {
    block: Block,
    block_coords: BlockCoords,
    block_light: SkyLight,
    chunk_class: &'a BlocksVec<BlockClass>,
    opacity_bcc: &'a OpacityBlockClassComponent,
    chunk_light: &'a mut BlocksVec<SkyLight>,
    queue: &'a mut VecDeque<(Block, BlockCoords)>,
}

// Assigns light to the neighbor blocks within the chunk
// Fills the queue with the neighbor blocks that will disperse light themselves next
impl LightDispersion<'_> {
    fn disperse(self) {
        let LightDispersion {
            block,
            block_coords,
            block_light,
            chunk_class,
            opacity_bcc,
            chunk_light,
            queue,
        } = self;

        let neighbors = block.same_chunk_neighbors(block_coords);

        for (side, neighbor) in neighbors.iter().enumerate() {
            if let Some((neighbor_block, neighbor_coords)) = neighbor {
                let neighbor_class = chunk_class.get(*neighbor_block);
                let neighbor_light = chunk_light.get_mut(*neighbor_block);

                match opacity_bcc.get(neighbor_class) {
                    Some(Opacity::Full) => {},
                    None => {
                        if side == 4 && block_light == SkyLight::MAX {
                            // Side index 4 is z_m (block below)
                            // we want max-level light to spread below indefinitely
                            *neighbor_light = SkyLight::MAX;
                            queue.push_back((*neighbor_block, *neighbor_coords));
                        } else {
                            let new_light = block_light.fade();

                            if new_light > SkyLight::MIN && new_light > *neighbor_light {
                                *neighbor_light = new_light;

                                queue.push_back((*neighbor_block, *neighbor_coords));
                            }
                        }
                    },
                }
            }
        }
    }
}

// Fills 1-block layers on each side with light from the neighbor chunks.
// As this is prerequisite process to light spill, chunk_light
// is intended to be filled with SkyLight::MIN.
struct AddSide<'a> {
    side: usize,
    old_chunk_light: Option<&'a BlocksVec<SkyLight>>,
    chunk_class: &'a BlocksVec<BlockClass>,
    opacity_bcc: &'a OpacityBlockClassComponent,
    chunk_light: &'a mut BlocksVec<SkyLight>,
    neighbor_chunks: [Option<(Chunk, &'a BlocksVec<BlockClass>, &'a BlocksVec<SkyLight>)>; 6],
    recalc_inner_blocks: &'a mut bool,
}

impl AddSide<'_> {
    fn run(self) {
        let Self {
            side,
            old_chunk_light,
            chunk_class,
            opacity_bcc,
            chunk_light,
            neighbor_chunks,
            recalc_inner_blocks,
        } = self;

        let (axis0, axis1, fixed_axis, fixed_axis_value, neighbor_fixed_axis_value) = match side {
            0 => (1, 2, 0, 0, BLOCKS_IN_CHUNK_EDGE - 1),
            1 => (1, 2, 0, BLOCKS_IN_CHUNK_EDGE - 1, 0),
            2 => (0, 2, 1, 0, BLOCKS_IN_CHUNK_EDGE - 1),
            3 => (0, 2, 1, BLOCKS_IN_CHUNK_EDGE - 1, 0),
            4 => (0, 1, 2, 0, BLOCKS_IN_CHUNK_EDGE - 1),
            5 => (0, 1, 2, BLOCKS_IN_CHUNK_EDGE - 1, 0),
            i => panic!("incorrect side index: {}", i),
        };

        for a0 in 0 .. BLOCKS_IN_CHUNK_EDGE {
            for a1 in 0 .. BLOCKS_IN_CHUNK_EDGE {
                let mut block_coords = [0; 3];

                block_coords[axis0] = a0;
                block_coords[axis1] = a1;
                block_coords[fixed_axis] = fixed_axis_value;

                let block = Block::from_coords(block_coords);

                let block_class = chunk_class.get(block);
                let block_light = chunk_light.get_mut(block);

                let old_block_light = old_chunk_light.as_ref().map(|c| *c.get(block));

                // TODO block transparency analysis
                if let Some(Opacity::Full) = opacity_bcc.get(block_class) {
                    *block_light = SkyLight::MIN;

                    if old_block_light != Some(*block_light) {
                        *recalc_inner_blocks = true;
                    }

                    continue;
                }

                let mut neighbor_block_coords = [0; 3];

                neighbor_block_coords[axis0] = a0;
                neighbor_block_coords[axis1] = a1;
                neighbor_block_coords[fixed_axis] = neighbor_fixed_axis_value;

                let neighbor_block = Block::from_coords(neighbor_block_coords);

                let neighbor_block_light = match &neighbor_chunks[side] {
                    Some((_chunks, _classes, light)) => *light.get(neighbor_block),
                    None => {
                        if side == SKY_SIDE {
                            SkyLight::MAX
                        } else {
                            SkyLight::MIN
                        }
                    },
                };

                let new_block_light = if side == SKY_SIDE && neighbor_block_light == SkyLight::MAX {
                    SkyLight::MAX
                } else {
                    neighbor_block_light.fade()
                };

                // Corners of the chunk should inherit the light from the brighter side
                *block_light = new_block_light.max(*block_light);

                if old_block_light != Some(*block_light) {
                    *recalc_inner_blocks = true;
                }
            }
        }
    }
}

// Checks if light in 1-block layers on the side changed and returns
// if neighbor chunk needs to be recalculated
struct CheckSide<'a> {
    side: usize,
    old_chunk_light: Option<&'a BlocksVec<SkyLight>>,
    chunk_light: &'a BlocksVec<SkyLight>,
    neighbor_chunks: [Option<(Chunk, &'a BlocksVec<BlockClass>, &'a BlocksVec<SkyLight>)>; 6],
    opacity_bcc: &'a OpacityBlockClassComponent,
}

impl CheckSide<'_> {
    fn needs_recalculation(self) -> Option<Chunk> {
        let Self {
            side,
            old_chunk_light,
            chunk_light,
            neighbor_chunks,
            opacity_bcc,
        } = self;

        let (axis0, axis1, fixed_axis, fixed_axis_value, neighbor_fixed_axis_value) = match side {
            0 => (1, 2, 0, 0, BLOCKS_IN_CHUNK_EDGE - 1),
            1 => (1, 2, 0, BLOCKS_IN_CHUNK_EDGE - 1, 0),
            2 => (0, 2, 1, 0, BLOCKS_IN_CHUNK_EDGE - 1),
            3 => (0, 2, 1, BLOCKS_IN_CHUNK_EDGE - 1, 0),
            4 => (0, 1, 2, 0, BLOCKS_IN_CHUNK_EDGE - 1),
            5 => (0, 1, 2, BLOCKS_IN_CHUNK_EDGE - 1, 0),
            i => panic!("incorrect side index: {}", i),
        };

        for a0 in 0 .. BLOCKS_IN_CHUNK_EDGE {
            for a1 in 0 .. BLOCKS_IN_CHUNK_EDGE {
                let mut block_coords = [0; 3];

                block_coords[axis0] = a0;
                block_coords[axis1] = a1;
                block_coords[fixed_axis] = fixed_axis_value;

                let block = Block::from_coords(block_coords);

                let new_block_light = chunk_light.get(block);
                let old_block_light = old_chunk_light.as_ref().map(|c| c.get(block));

                let mut neighbor_block_coords = [0; 3];

                neighbor_block_coords[axis0] = a0;
                neighbor_block_coords[axis1] = a1;
                neighbor_block_coords[fixed_axis] = neighbor_fixed_axis_value;

                let neighbor_block = Block::from_coords(neighbor_block_coords);

                let (neighbor_chunk, neighbor_block_class, neighbor_block_light) =
                    if let Some((chunk, classes, light)) = &neighbor_chunks[side] {
                        (
                            chunk,
                            classes.get(neighbor_block),
                            light.get(neighbor_block),
                        )
                    } else {
                        continue;
                    };

                let neighbor_transparent = match opacity_bcc.get(neighbor_block_class) {
                    Some(Opacity::Full) => false,
                    None => true,
                };

                // Light levels differ, we should recalculate
                if old_block_light != Some(new_block_light)
                    // ... unless the neighbor block is opaque and will not pass the light anyway
                    // TODO neighbor block transparency check
                    && neighbor_transparent
                    // ... and unless the neighbor is on NOT ground side and light is already MAX,
                    // if the neighbor block is on the opposite side from the sky,
                    // it can only receive direct (MAX) light from us
                    && (side == GROUND_SIDE || *neighbor_block_light != SkyLight::MAX)
                {
                    return Some(*neighbor_chunk);
                }
            }
        }

        None
    }
}
