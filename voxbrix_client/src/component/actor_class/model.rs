use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::entity::actor_model::ActorModel;

pub type ModelActorClassComponent = OverridableActorClassComponent<Option<ActorModel>>;

impl OverridableFromDescriptor for ActorModel {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_model";
}
