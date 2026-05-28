use crate::component::{
    actor::WithUpdate,
    actor_class::PackableOverridableActorClassComponent,
};
use voxbrix_common::component::actor_class::sight::Sight;

impl WithUpdate for Sight {
    const UPDATE: &str = "actor_sight";
}

pub type SightActorClassComponent = PackableOverridableActorClassComponent<Sight>;
