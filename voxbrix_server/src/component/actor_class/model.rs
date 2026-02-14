use crate::component::{
    actor::WithUpdate,
    actor_class::PackableOverridableActorClassComponent,
};
use voxbrix_common::entity::actor_model::ActorModel;

impl WithUpdate for ActorModel {
    const UPDATE: &str = "actor_model";
}

pub type ModelActorClassComponent = PackableOverridableActorClassComponent<Option<ActorModel>>;
