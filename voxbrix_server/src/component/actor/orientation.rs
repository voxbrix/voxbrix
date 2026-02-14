use crate::component::actor::{
    ActorComponentPackable,
    WithUpdate,
};
use voxbrix_common::component::actor::orientation::Orientation;

impl WithUpdate for Orientation {
    const UPDATE: &str = "actor_orientation";
}

pub type OrientationActorComponent = ActorComponentPackable<Orientation>;
