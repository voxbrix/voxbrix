use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::component::actor_class::dimension_acceleration::DimensionAcceleration;

pub type DimensionAccelerationActorClassComponent =
    OverridableActorClassComponent<DimensionAcceleration>;

impl OverridableFromDescriptor for DimensionAcceleration {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_dimension_acceleration";
}
