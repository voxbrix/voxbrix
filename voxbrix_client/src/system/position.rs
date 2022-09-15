use crate::{
    component::{
        actor::{
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        block::class::ClassBlockComponent,
    },
    entity::{
        block::Block,
        chunk::Chunk,
    },
};
use either::Either;
use std::{
    cmp::Ordering,
    time::Duration,
};

const COLLISION_PUSHBACK: f32 = 1.0e-6;

pub struct PositionSystem {
    collider_blocks: [Vec<[i32; 3]>; 3],
}

impl PositionSystem {
    pub fn new() -> Self {
        Self {
            collider_blocks: [Vec::new(), Vec::new(), Vec::new()],
        }
    }

    pub fn process(
        &mut self,
        dt: Duration,
        cbc: &ClassBlockComponent,
        pc: &mut PositionActorComponent,
        vc: &VelocityActorComponent,
    ) {
        #[derive(Copy, Clone)]
        enum MoveDirection {
            Positive,
            Negative,
        }

        let h_radius = 0.45;
        let v_radius = 0.95;
        let zero_chunk = Chunk {
            position: [0, 0, 0],
            dimention: 0,
        };

        for (actor, velocity) in vc.iter() {
            if let Some(position) = pc.get_mut(actor) {
                let travel = velocity.clone() * dt;

                let radius = [h_radius, h_radius, v_radius];

                let axis_set = [(0, 1, 2), (1, 0, 2), (2, 0, 1)];

                self.collider_blocks.iter_mut().for_each(|c| c.clear());

                // Distance to collision in blocks by each axis
                let mut col_by_axis = [None; 3];
                let mut move_dir_by_axis = [None; 3];

                for (a0, a1, a2) in axis_set {
                    let move_dir = match travel.vector[a0].total_cmp(&0.0) {
                        Ordering::Greater => MoveDirection::Positive,
                        Ordering::Less => MoveDirection::Negative,
                        Ordering::Equal => continue,
                    };

                    move_dir_by_axis[a0] = Some(move_dir);

                    let (actor_start, block_offset) = match move_dir {
                        MoveDirection::Positive => (position.vector[a0] + radius[a0], 0),
                        MoveDirection::Negative => (position.vector[a0] - radius[a0], 1),
                    };

                    let block_start = actor_start.floor() as i32;

                    let actor_finish = actor_start + travel.vector[a0];

                    let block_finish = actor_finish.floor() as i32;

                    let block_range = match move_dir {
                        MoveDirection::Positive => Either::Left(block_start + 1 ..= block_finish),
                        MoveDirection::Negative => {
                            Either::Right((block_finish .. block_start).rev())
                        },
                    };

                    'axis: for block_a0 in block_range {
                        let t =
                            ((block_a0 + block_offset) as f32 - actor_start) / velocity.vector[a0];

                        let actor_a1 = position.vector[a1] + velocity.vector[a1] * t;

                        let block_a1m = (actor_a1 - radius[a1]).floor() as i32;
                        let block_a1p = (actor_a1 + radius[a1]).floor() as i32;

                        for block_a1 in block_a1m ..= block_a1p {
                            let actor_a2 = position.vector[a2] + velocity.vector[a2] * t;

                            let block_a2m = (actor_a2 - radius[a2]).floor() as i32;
                            let block_a2p = (actor_a2 + radius[a2]).floor() as i32;

                            for block_a2 in block_a2m ..= block_a2p {
                                let mut chunk_offset = [0; 3];
                                chunk_offset[a0] = block_a0;
                                chunk_offset[a1] = block_a1;
                                chunk_offset[a2] = block_a2;
                                let (chunk, block) =
                                    Block::from_chunk_offset(zero_chunk, chunk_offset);

                                if let Some(blocks) = cbc.get_chunk(&chunk) {
                                    let block_class = blocks.get(block);

                                    // TODO better block analysis
                                    if block_class.is_some() && block_class.unwrap().0 == 1 {
                                        // Collision!
                                        // TODO:
                                        // record collision time t_min and/or actor coordinate
                                        // for comparison with other axis collision detection
                                        // after that we can break from detection by this axis.
                                        // optimization: we can quit comparing in subsequent
                                        // axis if t > t_min

                                        // pos_col_min[a0] = Some((block_a0 + block_offset) as f32 + match move_dir {
                                        // MoveDirection::Positive => - a0_radius - COLLISION_PUSHBACK,
                                        // MoveDirection::Negative => a0_radius + COLLISION_PUSHBACK,
                                        // });

                                        self.collider_blocks[a0].push(chunk_offset);
                                        col_by_axis[a0] = Some(block_a0);

                                        // break 'colliders;
                                    }
                                } else {
                                    // TODO chunk not loaded
                                }
                            }
                        }

                        if col_by_axis[a0].is_some() {
                            break 'axis;
                        }
                    }
                }

                let mut pos = position.clone() + travel;

                for (axis, chunk_offsets) in self.collider_blocks.iter_mut().enumerate() {
                    // Filter out surfaces of the blocks that can cause actor stuck
                    // when it moves at an angle to the smooth surface that
                    // consists of multiple blocks
                    //
                    // Essentially, we ignore the diagonal blocks (x) for an actor (a):
                    // |x|   |x|
                    // | |a a| |
                    // | |a a| |
                    // |x|   |x|
                    let collider_chunk_offset = chunk_offsets.drain(..).find(|chunk_offset| {
                        let ignore_iter = col_by_axis.iter().enumerate().filter_map(|(ia, io)| {
                            if ia != axis {
                                Some((ia, io.as_ref()?))
                            } else {
                                None
                            }
                        });

                        for (ignore_axis, ignore_offset) in ignore_iter {
                            if chunk_offset[ignore_axis] == *ignore_offset {
                                return false;
                            }
                        }
                        return true;
                    });

                    let collider_chunk_offset = match collider_chunk_offset {
                        Some(o) => o,
                        None => continue,
                    };

                    let move_dir = match move_dir_by_axis[axis] {
                        Some(o) => o,
                        None => continue,
                    };

                    pos.vector[axis] = match move_dir {
                        MoveDirection::Positive => {
                            collider_chunk_offset[axis] as f32 - radius[axis] - COLLISION_PUSHBACK
                        },
                        MoveDirection::Negative => {
                            (collider_chunk_offset[axis] + 1) as f32
                                + radius[axis]
                                + COLLISION_PUSHBACK
                        },
                    };
                }

                *position = pos;
            }
        }
    }
}
