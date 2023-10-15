use crate::component::actor::ActorComponentPackable;
use std::time::Instant;
use voxbrix_common::component::actor::orientation::Orientation;

pub struct TargetOrientation {
    pub receive_time: Instant,
    pub starting_orientation: Orientation,
    pub target_orientation: Orientation,
}

pub type TargetOrientationActorComponent = ActorComponentPackable<TargetOrientation>;
