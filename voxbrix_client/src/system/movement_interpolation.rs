use crate::component::actor::{
    orientation::OrientationActorComponent,
    position::PositionActorComponent,
    target_orientation::TargetOrientationActorComponent,
    target_position::TargetPositionActorComponent,
    TargetQueue,
    Writable,
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
        chunk::ChunkPositionOperations,
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
        let current_time = Instant::now() - SERVER_TICK_INTERVAL * TARGET_QUEUE_LENGTH_U32;

        for (actor, target_queue) in target_position_ac.iter_mut() {
            let mut position = match position_ac.get_writable(&actor, snapshot) {
                Some(v) => v,
                None => continue,
            };

            if let Some((target_position, time_left)) =
                Self::find_next_target(target_queue, &mut position, current_time)
            {
                let completion = (SERVER_TICK_INTERVAL - time_left).as_secs_f32()
                    / SERVER_TICK_INTERVAL.as_secs_f32();

                let starting = target_queue.starting;

                if target_position.chunk.dimension != starting.chunk.dimension {
                    position.update(target_position.clone());
                    continue;
                }

                let chunk_offset: Vec3F32 = target_position
                    .chunk
                    .position
                    .checked_sub(starting.chunk.position)
                    .expect("should not fail")
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

        for (actor, target_queue) in target_orientation_ac.iter_mut() {
            let mut orientation = match orientation_ac.get_writable(&actor, snapshot) {
                Some(v) => v,
                None => continue,
            };

            if let Some((target_orientation, time_left)) =
                Self::find_next_target(target_queue, &mut orientation, current_time)
            {
                let completion = (SERVER_TICK_INTERVAL - time_left).as_secs_f32()
                    / SERVER_TICK_INTERVAL.as_secs_f32();

                let rotation = target_queue
                    .starting
                    .rotation
                    .lerp(target_orientation.rotation, completion);

                orientation.update(Orientation { rotation });
            }
        }
    }

    /// Returns the next target and the time left to reach it.
    fn find_next_target<T>(
        target_queue: &mut TargetQueue<T>,
        value: &mut Writable<T>,
        current_time: Instant,
    ) -> Option<(T, Duration)>
    where
        T: PartialEq + Copy,
    {
        let TargetQueue {
            starting,
            target_queue,
        } = target_queue;

        while let Some(target_orientation) = target_queue.first().copied() {
            let time_left = target_orientation
                .reach_time
                .saturating_duration_since(current_time);

            if !time_left.is_zero() {
                if time_left <= SERVER_TICK_INTERVAL {
                    // This target is NOT too far in the future
                    return Some((target_orientation.value, time_left));
                }

                break;
            }

            // This target is already reached
            *starting = target_orientation.value;
            value.update(*starting);
            target_queue.pop_at(0);
        }

        None
    }
}
