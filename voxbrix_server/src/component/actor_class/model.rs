use crate::component::actor_class::{
    PackableOverridableActorClassComponent,
    WithUpdate,
};
use voxbrix_common::entity::actor_model::ActorModel;

impl WithUpdate for ActorModel {
    const UPDATE: &str = "actor_model";
}

pub type ModelActorClassComponent = PackableOverridableActorClassComponent<Option<ActorModel>>;
