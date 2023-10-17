use crate::component::actor::{
    orientation::OrientationActorComponent,
    position::PositionActorComponent,
    target_orientation::TargetOrientationActorComponent,
    target_position::TargetPositionActorComponent,
    TargetQueue,
};
use std::time::{
    Duration,
    Instant,
};
use voxbrix_common::{
    component::actor::{
        orientation::Orientation,
        position::Position,
    },
    entity::{
        block::BLOCKS_IN_CHUNK_EDGE_F32,
        snapshot::Snapshot,
    },
    math::Vec3F32,
};

const SERVER_TICK_INTERVAL: Duration = Duration::from_millis(50);
pub const TARGET_QUEUE_LENGTH: usize = 2;
pub const TARGET_QUEUE_LENGTH_U32: u32 = TARGET_QUEUE_LENGTH as u32;

pub struct MovementInterpolationSystem;

impl MovementInterpolationSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn process(
        &mut self,
        target_position_ac: &mut TargetPositionActorComponent,
        target_orientation_ac: &mut TargetOrientationActorComponent,
        position_ac: &mut PositionActorComponent,
        orientation_ac: &mut OrientationActorComponent,
        snapshot: Snapshot,
    ) {
        for (
            actor,
            TargetQueue {
                starting,
                target_queue,
            },
        ) in target_position_ac.iter_mut()
        {
            let mut position = match position_ac.get_writable(&actor, snapshot) {
                Some(v) => v,
                None => continue,
            };

            let current_time = Instant::now() - SERVER_TICK_INTERVAL * TARGET_QUEUE_LENGTH_U32;

            let mut result = None;

            while let Some(target_position) = target_queue.first().copied() {
                let time_left = target_position
                    .reach_time
                    .saturating_duration_since(current_time);

                if !time_left.is_zero() {
                    if time_left <= SERVER_TICK_INTERVAL {
                        // This target is NOT too far in the future
                        result = Some((target_position, time_left));
                    }

                    break;
                }

                // This target is already reached
                *starting = target_position.value;
                position.update(*starting);
                target_queue.pop_at(0);
            }

            if let Some((target_position, time_left)) = result {
                let completion = (SERVER_TICK_INTERVAL - time_left).as_secs_f32()
                    / SERVER_TICK_INTERVAL.as_secs_f32();

                let target_position = target_position.value;

                if target_position.chunk.dimension != starting.chunk.dimension {
                    position.update(target_position.clone());
                    continue;
                }

                let chunk_offset: Vec3F32 = (target_position.chunk.position
                    - starting.chunk.position)
                    .to_array()
                    .map(|i| i as f32 * BLOCKS_IN_CHUNK_EDGE_F32)
                    .into();

                let from_start_to_target =
                    chunk_offset + (target_position.offset - starting.offset);

                let position_offset = (target_position.offset - from_start_to_target)
                    .lerp(target_position.offset, completion);

                position.update(Position {
                    chunk: target_position.chunk,
                    offset: position_offset,
                });
            }
        }

        for (
            actor,
            TargetQueue {
                starting,
                target_queue,
            },
        ) in target_orientation_ac.iter_mut()
        {
            let mut orientation = match orientation_ac.get_writable(&actor, snapshot) {
                Some(v) => v,
                None => continue,
            };

            let current_time = Instant::now() - SERVER_TICK_INTERVAL * TARGET_QUEUE_LENGTH_U32;

            let mut result = None;

            while let Some(target_orientation) = target_queue.first().copied() {
                let time_left = target_orientation
                    .reach_time
                    .saturating_duration_since(current_time);

                if !time_left.is_zero() {
                    if time_left <= SERVER_TICK_INTERVAL {
                        // This target is NOT too far in the future
                        result = Some((target_orientation, time_left));
                    }

                    break;
                }

                // This target is already reached
                *starting = target_orientation.value;
                orientation.update(*starting);
                target_queue.pop_at(0);
            }

            if let Some((target_orientation, time_left)) = result {
                let completion = (SERVER_TICK_INTERVAL - time_left).as_secs_f32()
                    / SERVER_TICK_INTERVAL.as_secs_f32();

                let target_orientation = target_orientation.value;

                let rotation = starting
                    .rotation
                    .lerp(target_orientation.rotation, completion);

                orientation.update(Orientation { rotation });
            }
        }
    }
}
