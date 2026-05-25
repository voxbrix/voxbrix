use crate::component::{
    actor::WithUpdate,
    actor_class::PackableOverridableActorClassComponent,
};
use voxbrix_common::component::actor_class::density::Density;

impl WithUpdate for Density {
    const UPDATE: &str = "actor_density";
}

pub type DensityActorClassComponent = PackableOverridableActorClassComponent<Density>;
