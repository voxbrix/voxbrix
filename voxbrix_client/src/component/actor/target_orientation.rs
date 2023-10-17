use crate::component::actor::{
    ActorComponentUnpackable,
    TargetQueue,
};
use voxbrix_common::component::actor::orientation::Orientation;

pub type TargetOrientationActorComponent = ActorComponentUnpackable<TargetQueue<Orientation>>;
