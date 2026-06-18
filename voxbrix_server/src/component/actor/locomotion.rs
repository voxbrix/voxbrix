use crate::component::actor::{
    ActorComponentPackable,
    WithUpdate,
};
use voxbrix_common::component::actor::locomotion::Locomotion;

impl WithUpdate for Locomotion {
    const UPDATE: &str = "actor_locomotion";
}

pub type LocomotionActorComponent = ActorComponentPackable<Locomotion>;
