use crate::component::actor::ActorComponentPackable;
use std::time::Instant;
use voxbrix_common::component::actor::position::Position;

pub struct TargetPosition {
    pub receive_time: Instant,
    pub starting_position: Position,
    pub target_position: Position,
}

pub type TargetPositionActorComponent = ActorComponentPackable<TargetPosition>;
