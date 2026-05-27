use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::component::actor_class::propulsion::Propulsion;

pub type PropulsionActorClassComponent = OverridableActorClassComponent<Propulsion>;

impl OverridableFromDescriptor for Propulsion {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_propulsion";
}
