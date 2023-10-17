use crate::component::actor::{
    ActorComponentUnpackable,
    TargetQueue,
};
use voxbrix_common::component::actor::position::Position;

pub type TargetPositionActorComponent = ActorComponentUnpackable<TargetQueue<Position>>;
