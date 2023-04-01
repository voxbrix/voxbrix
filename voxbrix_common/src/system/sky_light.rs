use crate::{
    component::block::{
        class::ClassBlockComponent,
        sky_light::{
            SkyLight,
            SkyLightBlockComponent,
        },
        BlocksVec,
    },
    entity::{
        block::{
            Block,
            BLOCKS_IN_CHUNK,
            BLOCKS_IN_CHUNK_EDGE,
        },
        block_class::BlockClass,
        chunk::Chunk,
    },
    math::Vec3,
};
use arrayvec::ArrayVec;
use std::collections::VecDeque;

const SKY_SIDE: usize = 5;
const GROUND_SIDE: usize = 4;

pub struct SkyLightSystem;

impl SkyLightSystem {
    pub fn new() -> Self {
        Self
    }

    /// Should only be called on existing chunk that has `ClassBlockComponent` defined,
    /// will panic otherwise.
    /// Returns the requested chunk and neighbor chunks that require recalculation.
    /// If the old light block component for the target chunk exists, it should be removed from
    /// the SkyLightBlockComponent structure and provided as argument to the function,
    /// the returned light block component should be inserted instead.
    pub fn recalculate_chunk(
        &self,
        chunk: Chunk,
        old_chunk_light: Option<BlocksVec<SkyLight>>,
        cbc: &ClassBlockComponent,
        slbc: &SkyLightBlockComponent,
    ) -> (BlocksVec<SkyLight>, ArrayVec<Chunk, 6>) {
        let mut queue = VecDeque::new();

        let chunk_class = cbc
            .get_chunk(&chunk)
            .expect("calculating light for existing chunk");

        let mut chunk_light = BlocksVec::new(vec![SkyLight::MIN; BLOCKS_IN_CHUNK]);

        let neighbor_chunk_ids = [
            Vec3::new(-1, 0, 0),
            Vec3::new(1, 0, 0),
            Vec3::new(0, -1, 0),
            Vec3::new(0, 1, 0),
            Vec3::new(0, 0, -1),
            Vec3::new(0, 0, 1),
        ]
        .map(|offset| chunk.offset(offset));

        let neighbor_chunks = neighbor_chunk_ids
            .into_iter()
            .map(|chunk| {
                let chunk = chunk?;
                let block_classes = cbc.get_chunk(&chunk)?;
                let block_light = slbc.get_chunk(&chunk)?;

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
                }
                .needs_recalculation()
            })
            .collect();

        (chunk_light, chunks_to_recalc)
    }
}

struct LightDispersion<'a> {
    block: Block,
    block_coords: [usize; 3],
    block_light: SkyLight,
    chunk_class: &'a BlocksVec<BlockClass>,
    chunk_light: &'a mut BlocksVec<SkyLight>,
    queue: &'a mut VecDeque<(Block, [usize; 3])>,
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
            chunk_light,
            queue,
        } = self;

        let neighbors = block.same_chunk_neighbors(block_coords);

        for (side, neighbor) in neighbors.iter().enumerate() {
            if let Some((neighbor_block, neighbor_coords)) = neighbor {
                let neighbor_class = chunk_class.get(*neighbor_block);
                let neighbor_light = chunk_light.get_mut(*neighbor_block);

                // TODO block transparency analysis
                if neighbor_class.0 == 1 {
                    // Do nothing
                } else if side == 4 && block_light == SkyLight::MAX {
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
            }
        }
    }
}

// Fills 1-block layers on each side with light from the neighbor chunks
struct AddSide<'a> {
    side: usize,
    old_chunk_light: Option<&'a BlocksVec<SkyLight>>,
    chunk_class: &'a BlocksVec<BlockClass>,
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
                if block_class.0 == 1 {
                    *block_light = SkyLight::MIN;

                    if old_block_light.is_none() || old_block_light != Some(*block_light) {
                        *recalc_inner_blocks = true;
                    }

                    continue;
                }

                let mut block_above_coords = [0; 3];

                block_above_coords[axis0] = a0;
                block_above_coords[axis1] = a1;
                block_above_coords[fixed_axis] = neighbor_fixed_axis_value;

                let block_above = Block::from_coords(block_above_coords);

                let block_above_light = match &neighbor_chunks[side] {
                    Some((_chunks, _classes, light)) => *light.get(block_above),
                    None => {
                        if side == SKY_SIDE {
                            SkyLight::MAX
                        } else {
                            SkyLight::MIN
                        }
                    },
                };

                *block_light = if side == SKY_SIDE && block_above_light == SkyLight::MAX {
                    SkyLight::MAX
                } else {
                    block_above_light.fade()
                };

                if old_block_light.is_none() || old_block_light != Some(*block_light) {
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
}

impl CheckSide<'_> {
    fn needs_recalculation(self) -> Option<Chunk> {
        let Self {
            side,
            old_chunk_light,
            chunk_light,
            neighbor_chunks,
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

                // Light levels differ, we should recalculate
                if old_block_light != Some(new_block_light)
                    // ... unless the neighbor block is opaque and will not pass the light anyway
                    // TODO neighbor block transparency check
                    && neighbor_block_class.0 != 1
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
