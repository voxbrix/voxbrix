use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::component::actor_class::density::Density;

pub type DensityActorClassComponent = OverridableActorClassComponent<Density>;

impl OverridableFromDescriptor for Density {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_density";
}
