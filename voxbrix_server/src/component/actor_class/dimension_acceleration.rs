use crate::component::{
    actor::WithUpdate,
    actor_class::PackableOverridableActorClassComponent,
};
use voxbrix_common::component::actor_class::dimension_acceleration::DimensionAcceleration;

impl WithUpdate for DimensionAcceleration {
    const UPDATE: &str = "actor_dimension_acceleration";
}

pub type DimensionAccelerationActorClassComponent =
    PackableOverridableActorClassComponent<DimensionAcceleration>;
