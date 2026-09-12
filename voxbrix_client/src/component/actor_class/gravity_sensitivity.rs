use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::component::actor_class::gravity_sensitivity::GravitySensitivity;

pub type GravitySensitivityActorClassComponent = OverridableActorClassComponent<GravitySensitivity>;

impl OverridableFromDescriptor for GravitySensitivity {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_gravity_sensitivity";
}
