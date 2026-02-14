use crate::component::{
    actor::WithUpdate,
    actor_class::PackableOverridableActorClassComponent,
};
use voxbrix_common::component::actor_class::health::Health;

impl WithUpdate for Health {
    const UPDATE: &str = "actor_health";
}

pub type HealthActorClassComponent = PackableOverridableActorClassComponent<Option<Health>>;
