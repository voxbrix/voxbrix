use crate::{
    component::{
        actor::{
            orientation::Orientation,
            position::{
                GlobalPosition,
                GlobalPositionActorComponent,
            },
            velocity::VelocityActorComponent,
        },
        block::class::ClassBlockComponent,
    },
    entity::{
        block::{
            Block,
            BLOCKS_IN_CHUNK_EDGE,
        },
        chunk::Chunk,
    },
};
use either::Either;
use std::{
    cmp::Ordering,
    time::Duration,
};
use voxbrix_common::math::Round;

const COLLISION_PUSHBACK: f32 = 1.0e-3;
const MAX_BLOCK_TARGET_DISTANCE: i32 = BLOCKS_IN_CHUNK_EDGE as i32;

pub struct PositionSystem;

impl PositionSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn process(
        &mut self,
        dt: Duration,
        cbc: &ClassBlockComponent,
        gpc: &mut GlobalPositionActorComponent,
        vc: &VelocityActorComponent,
    ) {
        #[derive(Copy, Clone)]
        enum MoveDirection {
            Positive,
            Negative,
        }

        let h_radius = 0.45;
        let v_radius = 0.95;

        for (actor, velocity) in vc.iter() {
            if let Some(GlobalPosition {
                chunk: center_chunk,
                offset: position,
            }) = gpc.get_mut(&actor)
            {
                let travel = velocity.clone() * dt;

                let radius = [h_radius, h_radius, v_radius];

                let axis_set = [(0, 1, 2), (1, 0, 2), (2, 0, 1)];

                // Distance to collision in blocks by each axis
                let mut max_movement = [None; 3];

                for (a0, a1, a2) in axis_set {
                    let move_dir = match travel.vector[a0].total_cmp(&0.0) {
                        Ordering::Greater => MoveDirection::Positive,
                        Ordering::Less => MoveDirection::Negative,
                        Ordering::Equal => continue,
                    };

                    let (actor_start, block_offset) = match move_dir {
                        MoveDirection::Positive => (position[a0] + radius[a0], 0),
                        MoveDirection::Negative => (position[a0] - radius[a0], 1),
                    };

                    let block_start = actor_start.round_down();

                    let actor_finish = actor_start + travel.vector[a0];

                    let block_finish = actor_finish.round_down();

                    let block_range = match move_dir {
                        MoveDirection::Positive => Either::Left(block_start + 1 ..= block_finish),
                        MoveDirection::Negative => {
                            Either::Right((block_finish .. block_start).rev())
                        },
                    };

                    'axis: for block_a0 in block_range {
                        let t =
                            ((block_a0 + block_offset) as f32 - actor_start) / velocity.vector[a0];

                        let actor_a1 = position[a1] + velocity.vector[a1] * t;

                        let block_a1m = (actor_a1 - radius[a1]).round_down();
                        let block_a1p = (actor_a1 + radius[a1]).round_down();

                        for block_a1 in block_a1m ..= block_a1p {
                            let actor_a2 = position[a2] + velocity.vector[a2] * t;

                            let block_a2m = (actor_a2 - radius[a2]).round_down();
                            let block_a2p = (actor_a2 + radius[a2]).round_down();

                            for block_a2 in block_a2m ..= block_a2p {
                                let mut chunk_offset = [0; 3];
                                chunk_offset[a0] = block_a0;
                                chunk_offset[a1] = block_a1;
                                chunk_offset[a2] = block_a2;
                                let (chunk, block) =
                                    Block::from_chunk_offset(*center_chunk, chunk_offset);

                                if let Some(block_class) =
                                    cbc.get_chunk(&chunk).and_then(|b| b.get(block))
                                {
                                    // TODO better block analysis
                                    if block_class.0 == 1 {
                                        // Collision!
                                        // Now check whether we should skip stopping in case the
                                        // border we hit is between two solid blocks ("before" block and
                                        // the target block) and actor is not inside "before"
                                        // block. Latter condition is requred to prevent actor from
                                        // falling/moving endlessly under textures in case they
                                        // somehow ended up inside the solid block (e.g. block was
                                        // built on the actor's place or the block is not cube
                                        // shape).
                                        let (block_before_a0, actor_current_front_a0) =
                                            match move_dir {
                                                MoveDirection::Positive => {
                                                    (
                                                        block_a0 - 1,
                                                        (position[a0] + radius[a0]).round_down(),
                                                    )
                                                },
                                                MoveDirection::Negative => {
                                                    (
                                                        block_a0 + 1,
                                                        (position[a0] - radius[a0]).round_down(),
                                                    )
                                                },
                                            };

                                        let actor_current_a1p =
                                            (position[a1] + radius[a1]).round_down();
                                        let actor_current_a1m =
                                            (position[a1] - radius[a1]).round_down();

                                        let actor_current_a2p =
                                            (position[a2] + radius[a2]).round_down();
                                        let actor_current_a2m =
                                            (position[a2] - radius[a2]).round_down();

                                        let stop = actor_current_front_a0 == block_before_a0
                                            && (actor_current_a1m ..= actor_current_a1p)
                                                .contains(&block_a1)
                                            && (actor_current_a2m ..= actor_current_a2p)
                                                .contains(&block_a2)
                                            || {
                                                let mut block_before = [0; 3];
                                                block_before[a0] = block_before_a0;
                                                block_before[a1] = block_a1;
                                                block_before[a2] = block_a2;

                                                let (chunk, block) = Block::from_chunk_offset(
                                                    *center_chunk,
                                                    block_before,
                                                );

                                                cbc.get_chunk(&chunk)
                                                    .and_then(|b| b.get(block))
                                                    .map(|bc| {
                                                        // Opposite of collision condition
                                                        // TODO better block analysis
                                                        !(bc.0 == 1)
                                                    })
                                                    // stop if chunk is not loaded
                                                    .unwrap_or(true)
                                            };

                                        if stop {
                                            max_movement[a0] = Some(
                                                (block_a0 + block_offset) as f32
                                                    + match move_dir {
                                                        MoveDirection::Positive => {
                                                            -radius[a0] - COLLISION_PUSHBACK
                                                        },
                                                        MoveDirection::Negative => {
                                                            radius[a0] + COLLISION_PUSHBACK
                                                        },
                                                    },
                                            );
                                        }
                                    }
                                } else {
                                    // TODO chunk not loaded
                                }
                            }
                        }

                        if max_movement[a0].is_some() {
                            break 'axis;
                        }
                    }
                }

                let mut new_position = position.clone() + travel.vector;

                for a in 0 ..= 2 {
                    if let Some(max_movement) = max_movement[a] {
                        new_position[a] = max_movement;
                    }
                }

                let new_chunk = new_position
                    .as_ref()
                    .iter()
                    .find(|dist| dist.abs() > BLOCKS_IN_CHUNK_EDGE as f32)
                    .is_some();

                if new_chunk {
                    let chunk_diff_vec =
                        new_position.map(|f| f as i32 / BLOCKS_IN_CHUNK_EDGE as i32);

                    let actor_diff_vec =
                        chunk_diff_vec.map(|i| i as f32 * BLOCKS_IN_CHUNK_EDGE as f32);

                    center_chunk.position = center_chunk.position + chunk_diff_vec;

                    new_position = new_position - actor_diff_vec;
                }

                *position = new_position;
            }
        }
    }

    pub fn get_target_block<F>(
        position: &GlobalPosition,
        orientation: &Orientation,
        mut targeting: F,
    ) -> Option<(Chunk, Block, usize)>
    where
        F: FnMut(Chunk, Block) -> bool,
    {
        let forward = orientation.forward();

        let mut time_block = None;

        for (axis_0, axis_1, axis_2) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
            for axis_offset in 0 .. MAX_BLOCK_TARGET_DISTANCE {
                // wall_offset helps to calculate the distance to the layer ("wall") of blocks
                //     if we move to positive direction we need to add 1 after round_down()
                //     while moving in the negative direction, the value is 0
                // block_coord_offset helps to get the coordinate of the "wall" block layer
                //     if we move to the negative direction the actual coordinate would be 1 block
                //     behind the "wall" coordinate, because we "collide" with the front side of
                //     the block in this case
                //     while moving in the positive direction, the value is 0 as we collide with
                //     the back side of the block, which is the same as it's coordinate
                // side_index is a index of side/neighbor in [x_m, x_p, y_m, y_p, z_m, z_p]
                let (axis_offset, wall_offset, block_coord_offset, side_index) =
                    match forward[axis_0].partial_cmp(&0.0) {
                        Some(Ordering::Less) => (-axis_offset, 0, -1, axis_0 * 2 + 1),
                        Some(Ordering::Greater) => (axis_offset, 1, 0, axis_0 * 2),
                        _ => continue,
                    };

                let block_side_axis_0 =
                    (position.offset[axis_0] + axis_offset as f32).round_down() + wall_offset;

                let time = (block_side_axis_0 as f32 - position.offset[axis_0]) / forward[axis_0];

                let block_axis_0 = block_side_axis_0 + block_coord_offset;

                let is_record = if let Some((old_time, _)) = time_block {
                    time < old_time
                } else {
                    true
                };

                if is_record {
                    let block_axis_1 =
                        (position.offset[axis_1] + time * forward[axis_1]).round_down();

                    let block_axis_2 =
                        (position.offset[axis_2] + time * forward[axis_2]).round_down();

                    let mut block_offset = [0; 3];

                    block_offset[axis_0] = block_axis_0;
                    block_offset[axis_1] = block_axis_1;
                    block_offset[axis_2] = block_axis_2;

                    let (chunk, block) = Block::from_chunk_offset(position.chunk, block_offset);

                    if targeting(chunk, block) {
                        time_block = Some((time, (chunk, block, side_index)));
                    }
                }
            }
        }

        Some(time_block?.1)
    }
}
