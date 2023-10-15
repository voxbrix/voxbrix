use crate::component::actor::{
    orientation::OrientationActorComponent,
    position::PositionActorComponent,
    target_orientation::{
        TargetOrientation,
        TargetOrientationActorComponent,
    },
    target_position::{
        TargetPosition,
        TargetPositionActorComponent,
    },
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

pub struct MovementInterpolationSystem;

impl MovementInterpolationSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn process(
        &mut self,
        target_position_ac: &TargetPositionActorComponent,
        target_orientation_ac: &TargetOrientationActorComponent,
        position_ac: &mut PositionActorComponent,
        orientation_ac: &mut OrientationActorComponent,
        snapshot: Snapshot,
    ) {
        for (
            actor,
            TargetPosition {
                receive_time,
                starting_position,
                target_position,
            },
        ) in target_position_ac.iter()
        {
            let mut position = match position_ac.get_writable(&actor, snapshot) {
                Some(v) => v,
                None => {
                    position_ac.insert(actor, target_position.clone(), snapshot);
                    continue;
                },
            };

            if target_position.chunk.dimension != starting_position.chunk.dimension {
                position.update(target_position.clone());
                continue;
            }

            let completion = (Instant::now()
                .saturating_duration_since(*receive_time)
                .as_secs_f32()
                / SERVER_TICK_INTERVAL.as_secs_f32())
            .min(1.0);

            let chunk_offset: Vec3F32 = (target_position.chunk.position
                - starting_position.chunk.position)
                .to_array()
                .map(|i| i as f32 * BLOCKS_IN_CHUNK_EDGE_F32)
                .into();

            let from_start_to_target =
                chunk_offset + (target_position.offset - starting_position.offset);

            let position_offset = (target_position.offset - from_start_to_target)
                .lerp(target_position.offset, completion);

            position.update(Position {
                chunk: target_position.chunk,
                offset: position_offset,
            });
        }

        for (
            actor,
            TargetOrientation {
                receive_time,
                starting_orientation,
                target_orientation,
            },
        ) in target_orientation_ac.iter()
        {
            let mut orientation = match orientation_ac.get_writable(&actor, snapshot) {
                Some(v) => v,
                None => {
                    orientation_ac.insert(actor, target_orientation.clone(), snapshot);
                    continue;
                },
            };

            let completion = (Instant::now()
                .saturating_duration_since(*receive_time)
                .as_secs_f32()
                / SERVER_TICK_INTERVAL.as_secs_f32())
            .min(1.0);

            let rotation = starting_orientation
                .rotation
                .lerp(target_orientation.rotation, completion);

            orientation.update(Orientation { rotation });
        }
    }
}
