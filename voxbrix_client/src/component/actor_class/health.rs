use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::component::actor_class::health::Health;

pub type HealthActorClassComponent = OverridableActorClassComponent<Option<Health>>;

impl OverridableFromDescriptor for Health {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_health";
}
