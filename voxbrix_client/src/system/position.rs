use arrayvec::ArrayVec;
use either::Either;
use std::{
    cmp::Ordering,
    time::Duration,
};
use voxbrix_common::{
    component::{
        actor::{
            orientation::Orientation,
            position::{
                Position,
                PositionActorComponent,
            },
            velocity::VelocityActorComponent,
        },
        block::class::ClassBlockComponent,
        block_class::collision::{
            Collision,
            CollisionBlockClassComponent,
        },
    },
    entity::{
        block::{
            Block,
            BLOCKS_IN_CHUNK_EDGE,
        },
        chunk::Chunk,
    },
    math::{
        Round,
        Vec3,
    },
};

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
        class_bc: &ClassBlockComponent,
        collision_bcc: &CollisionBlockClassComponent,
        gpc: &mut PositionActorComponent,
        vc: &VelocityActorComponent,
    ) {
        enum MoveDirection {
            Positive,
            Negative,
        }

        struct MoveLimit {
            axis_set: [usize; 3],

            // powi(2) of the distance from the actor to the colliding block
            // defines priority of the move limits
            collider_distance: f32,
            max_movement: f32,
        }

        let h_radius = 0.45;
        let v_radius = 0.95;

        for (actor, velocity) in vc.iter() {
            if let Some(Position {
                chunk: center_chunk,
                offset: start_position,
            }) = gpc.get_mut(&actor)
            {
                let radius = [h_radius, h_radius, v_radius];

                let calc_pass = |finish_position: Vec3<f32>, axis_set: [usize; 3]| {
                    let [a0, a1, a2] = axis_set;

                    let travel_a0 = finish_position[a0] - start_position[a0];

                    let move_dir = match travel_a0.total_cmp(&0.0) {
                        Ordering::Greater => MoveDirection::Positive,
                        Ordering::Less => MoveDirection::Negative,
                        Ordering::Equal => return None,
                    };

                    let (actor_start, actor_finish, block_offset) = match move_dir {
                        MoveDirection::Positive => {
                            (
                                start_position[a0] + radius[a0],
                                finish_position[a0] + radius[a0],
                                0,
                            )
                        },
                        MoveDirection::Negative => {
                            (
                                start_position[a0] - radius[a0],
                                finish_position[a0] - radius[a0],
                                1,
                            )
                        },
                    };

                    let block_start = actor_start.round_down();
                    let block_finish = actor_finish.round_down();

                    let block_range = match move_dir {
                        MoveDirection::Positive => Either::Left(block_start + 1 ..= block_finish),
                        MoveDirection::Negative => {
                            Either::Right((block_finish .. block_start).rev())
                        },
                    };

                    for block_a0 in block_range {
                        let t =
                            ((block_a0 + block_offset) as f32 - actor_start) / velocity.vector[a0];

                        let actor_a1 = match velocity.vector[a1].total_cmp(&0.0) {
                            Ordering::Greater => {
                                (start_position[a1] + velocity.vector[a1] * t)
                                    .min(finish_position[a1])
                            },
                            Ordering::Less => {
                                (start_position[a1] + velocity.vector[a1] * t)
                                    .max(finish_position[a1])
                            },
                            Ordering::Equal => finish_position[a1],
                        };

                        let block_a1m = (actor_a1 - radius[a1]).round_down();
                        let block_a1p = (actor_a1 + radius[a1]).round_down();

                        for block_a1 in block_a1m ..= block_a1p {
                            let actor_a2 = match velocity.vector[a2].total_cmp(&0.0) {
                                Ordering::Greater => {
                                    (start_position[a2] + velocity.vector[a2] * t)
                                        .min(finish_position[a2])
                                },
                                Ordering::Less => {
                                    (start_position[a2] + velocity.vector[a2] * t)
                                        .max(finish_position[a2])
                                },
                                Ordering::Equal => finish_position[a2],
                            };

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
                                    class_bc.get_chunk(&chunk).map(|b| b.get(block))
                                {
                                    if let Some(collision) = collision_bcc.get(*block_class) {
                                        match collision {
                                            Collision::SolidCube => {
                                                return Some(MoveLimit {
                                                    axis_set,
                                                    collider_distance: (block_a0 as f32 + 0.5
                                                        - start_position[a0])
                                                        .powi(2)
                                                        + (block_a1 as f32 + 0.5
                                                            - start_position[a1])
                                                            .powi(2)
                                                        + (block_a2 as f32 + 0.5
                                                            - start_position[a2])
                                                            .powi(2),
                                                    max_movement: (block_a0 + block_offset) as f32
                                                        + match move_dir {
                                                            MoveDirection::Positive => {
                                                                -radius[a0] - COLLISION_PUSHBACK
                                                            },
                                                            MoveDirection::Negative => {
                                                                radius[a0] + COLLISION_PUSHBACK
                                                            },
                                                        },
                                                });
                                            },
                                        }
                                    }
                                } else {
                                    // TODO chunk not loaded
                                }
                            }
                        }
                    }

                    None
                };

                let mut finish_position = *start_position + (velocity.clone() * dt).vector;

                let axis_sets = [[0, 1, 2], [1, 0, 2], [2, 0, 1]];

                let mut move_limits = ArrayVec::<_, 3>::new();

                // Initial movement limiters by axis
                for axis_set in axis_sets {
                    if let Some(ml) = calc_pass(finish_position, axis_set) {
                        move_limits.push(ml);
                    }
                }

                // Re-calculation in case some colliding blocks are actually unreachable
                // behind other colliding blocks on different axis, priority is defined
                // by the distance from initial actor position to the colliding block
                while move_limits.len() > 1 {
                    move_limits.sort_unstable_by(|ml1, ml2| {
                        ml1.collider_distance.total_cmp(&ml2.collider_distance)
                    });

                    let mut move_limits_iter = move_limits.iter();

                    if let Some(move_limit) = move_limits_iter.next() {
                        finish_position[move_limit.axis_set[0]] = move_limit.max_movement;
                    }

                    let mut next_move_limits = ArrayVec::new();

                    for &MoveLimit { axis_set, .. } in move_limits_iter {
                        if let Some(ml) = calc_pass(finish_position, axis_set) {
                            next_move_limits.push(ml);
                        }
                    }

                    move_limits = next_move_limits;
                }

                if let Some(move_limit) = move_limits.first() {
                    finish_position[move_limit.axis_set[0]] = move_limit.max_movement;
                }

                // If we need to "move" actor to other chunk
                if finish_position
                    .as_ref()
                    .iter()
                    .any(|dist| dist.abs() > BLOCKS_IN_CHUNK_EDGE as f32)
                {
                    let chunk_diff_vec =
                        finish_position.map(|f| f as i32 / BLOCKS_IN_CHUNK_EDGE as i32);

                    let actor_diff_vec =
                        chunk_diff_vec.map(|i| i as f32 * BLOCKS_IN_CHUNK_EDGE as f32);

                    center_chunk.position = center_chunk.position + chunk_diff_vec;

                    finish_position = finish_position - actor_diff_vec;
                }

                *start_position = finish_position;
            }
        }
    }

    pub fn get_target_block<F>(
        position: &Position,
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
                    match forward[axis_0].total_cmp(&0.0) {
                        Ordering::Less => (-axis_offset, 0, -1, axis_0 * 2 + 1),
                        Ordering::Greater => (axis_offset, 1, 0, axis_0 * 2),
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
