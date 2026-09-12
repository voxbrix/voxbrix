use crate::component::{
    actor::WithUpdate,
    actor_class::PackableOverridableActorClassComponent,
};
use voxbrix_common::component::actor_class::gravity_sensitivity::GravitySensitivity;

impl WithUpdate for GravitySensitivity {
    const UPDATE: &str = "actor_gravity_sensitivity";
}

pub type GravitySensitivityActorClassComponent =
    PackableOverridableActorClassComponent<GravitySensitivity>;
