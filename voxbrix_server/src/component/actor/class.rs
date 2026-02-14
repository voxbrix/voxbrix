use crate::component::actor::{
    ActorComponentPackable,
    WithUpdate,
};
use voxbrix_common::entity::actor_class::ActorClass;

impl WithUpdate for ActorClass {
    const UPDATE: &str = "actor_class";
}

pub type ClassActorComponent = ActorComponentPackable<ActorClass>;
