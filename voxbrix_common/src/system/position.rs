use crate::{
    component::{
        actor::{
            position::Position,
            velocity::Velocity,
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
            BLOCKS_IN_CHUNK_EDGE_F32,
            BLOCKS_IN_CHUNK_EDGE_I32,
        },
        chunk::{
            Chunk,
            ChunkPositionOperations,
        },
    },
    math::{
        Round,
        Vec3F32,
    },
};
use arrayvec::ArrayVec;
use std::{
    cmp::Ordering,
    time::Duration,
};

const COLLISION_PUSHBACK: f32 = 1.0e-3;
const MAX_BLOCK_TARGET_DISTANCE: i32 = 8;

enum MoveDirection {
    Negative,
    Positive,
}

struct MoveLimit {
    axis_set: [usize; 3],

    // powi(2) of the distance from the actor to the colliding block
    // defines priority of the move limits
    collider_distance: f32,
    max_movement: f32,
}

enum BlockAxisRange<N, P> {
    Negative(N),
    Positive(P),
}

impl<N, P> Iterator for BlockAxisRange<N, P>
where
    N: Iterator<Item = i32>,
    P: Iterator<Item = i32>,
{
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            BlockAxisRange::Negative(iter) => iter.next(),
            BlockAxisRange::Positive(iter) => iter.next(),
        }
    }
}

pub fn process_actor(
    dt: Duration,
    class_bc: &ClassBlockComponent,
    collision_bcc: &CollisionBlockClassComponent,
    position: &Position,
    velocity: &Velocity,
    radius: &[f32; 3],
) -> Position {
    let Position {
        chunk: mut center_chunk,
        offset: mut start_position,
    } = position;

    let calc_pass = |finish_position: Vec3F32, axis_set: [usize; 3]| {
        let [a0, a1, a2] = axis_set;

        let travel_a0 = finish_position[a0] - start_position[a0];

        let move_dir = match travel_a0.total_cmp(&0.0) {
            Ordering::Less => MoveDirection::Negative,
            Ordering::Greater => MoveDirection::Positive,
            Ordering::Equal => return None,
        };

        let (actor_start, actor_finish, block_offset) = match move_dir {
            MoveDirection::Negative => {
                (
                    start_position[a0] - radius[a0],
                    finish_position[a0] - radius[a0],
                    1,
                )
            },
            MoveDirection::Positive => {
                (
                    start_position[a0] + radius[a0],
                    finish_position[a0] + radius[a0],
                    0,
                )
            },
        };

        let block_start = actor_start.round_down();
        let block_finish = actor_finish.round_down();

        let block_range = match move_dir {
            MoveDirection::Negative => {
                BlockAxisRange::Negative((block_finish .. block_start).rev())
            },
            MoveDirection::Positive => BlockAxisRange::Positive(block_start + 1 ..= block_finish),
        };

        for block_a0 in block_range {
            let t = ((block_a0 + block_offset) as f32 - actor_start) / velocity.vector[a0];

            let actor_a1 = match velocity.vector[a1].total_cmp(&0.0) {
                Ordering::Less => {
                    (start_position[a1] + velocity.vector[a1] * t).max(finish_position[a1])
                },
                Ordering::Greater => {
                    (start_position[a1] + velocity.vector[a1] * t).min(finish_position[a1])
                },
                Ordering::Equal => finish_position[a1],
            };

            let block_a1m = (actor_a1 - radius[a1]).round_down();
            let block_a1p = (actor_a1 + radius[a1]).round_down();

            for block_a1 in block_a1m ..= block_a1p {
                let actor_a2 = match velocity.vector[a2].total_cmp(&0.0) {
                    Ordering::Less => {
                        (start_position[a2] + velocity.vector[a2] * t).max(finish_position[a2])
                    },
                    Ordering::Greater => {
                        (start_position[a2] + velocity.vector[a2] * t).min(finish_position[a2])
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
                    if let Some((chunk, block)) =
                        Block::from_chunk_offset(center_chunk, chunk_offset)
                    {
                        if let Some(block_class) = class_bc.get_chunk(&chunk).map(|b| b.get(block))
                        {
                            if let Some(collision) = collision_bcc.get(block_class) {
                                match collision {
                                    Collision::SolidCube => {
                                        return Some(MoveLimit {
                                            axis_set,
                                            collider_distance: (block_a0 as f32 + 0.5
                                                - start_position[a0])
                                                .powi(2)
                                                + (block_a1 as f32 + 0.5 - start_position[a1])
                                                    .powi(2)
                                                + (block_a2 as f32 + 0.5 - start_position[a2])
                                                    .powi(2),
                                            max_movement: (block_a0 + block_offset) as f32
                                                + match move_dir {
                                                    MoveDirection::Negative => {
                                                        radius[a0] + COLLISION_PUSHBACK
                                                    },
                                                    MoveDirection::Positive => {
                                                        -radius[a0] - COLLISION_PUSHBACK
                                                    },
                                                },
                                        });
                                    },
                                }
                            }
                        } else {
                            // TODO chunk not loaded
                        }
                    } else {
                        // TODO chunk out of boundaries
                    }
                }
            }
        }

        None
    };

    let mut finish_position = start_position + (velocity.clone() * dt).vector;

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
        move_limits
            .sort_unstable_by(|ml1, ml2| ml1.collider_distance.total_cmp(&ml2.collider_distance));

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
        .any(|dist| dist.abs() > BLOCKS_IN_CHUNK_EDGE_F32)
    {
        let chunk_diff = finish_position
            .to_array()
            .map(|f| f as i32 / BLOCKS_IN_CHUNK_EDGE_I32);

        let final_chunk = chunk_diff.saturating_add(center_chunk.position);

        let actor_diff_vec: Vec3F32 = final_chunk
            .checked_sub(center_chunk.position)
            .expect("cannot fail")
            .map(|i| i as f32 * BLOCKS_IN_CHUNK_EDGE_F32)
            .into();

        center_chunk.position = final_chunk;

        finish_position = finish_position - actor_diff_vec;
    }

    start_position = finish_position;

    Position {
        chunk: center_chunk,
        offset: start_position,
    }
}

pub fn get_target_block(
    position: &Position,
    direction: Vec3F32,
    mut targeting: impl FnMut(Chunk, Block) -> bool,
) -> Option<(Chunk, Block, usize)> {
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
                match direction[axis_0].total_cmp(&0.0) {
                    Ordering::Less => (-axis_offset, 0, -1, axis_0 * 2 + 1),
                    Ordering::Greater => (axis_offset, 1, 0, axis_0 * 2),
                    _ => continue,
                };

            // Distance to the colliding side
            let block_side_axis_0 =
                position.offset[axis_0].round_down() + axis_offset + wall_offset;

            let time = (block_side_axis_0 as f32 - position.offset[axis_0]) / direction[axis_0];

            if time * direction.length() > MAX_BLOCK_TARGET_DISTANCE as f32 {
                break;
            }

            // Distance to the colliding block
            let block_axis_0 = block_side_axis_0 + block_coord_offset;

            let is_record = if let Some((old_time, _)) = time_block {
                time < old_time
            } else {
                true
            };

            if is_record {
                let block_axis_1 =
                    (position.offset[axis_1] + time * direction[axis_1]).round_down();

                let block_axis_2 =
                    (position.offset[axis_2] + time * direction[axis_2]).round_down();

                let mut block_offset = [0; 3];

                block_offset[axis_0] = block_axis_0;
                block_offset[axis_1] = block_axis_1;
                block_offset[axis_2] = block_axis_2;

                if let Some((chunk, block)) = Block::from_chunk_offset(position.chunk, block_offset)
                {
                    if targeting(chunk, block) {
                        time_block = Some((time, (chunk, block, side_index)));
                    }
                }
            }
        }
    }

    Some(time_block?.1)
}
