use crate::component::actor::{
    ActorComponentPackable,
    WithUpdate,
};
use voxbrix_common::component::actor::velocity::Velocity;

impl WithUpdate for Velocity {
    const UPDATE: &str = "actor_velocity";
}

pub type VelocityActorComponent = ActorComponentPackable<Velocity>;
