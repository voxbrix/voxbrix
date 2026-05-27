use crate::component::{
    actor::WithUpdate,
    actor_class::PackableOverridableActorClassComponent,
};
use voxbrix_common::component::actor_class::propulsion::Propulsion;

impl WithUpdate for Propulsion {
    const UPDATE: &str = "actor_propulsion";
}

pub type PropulsionActorClassComponent = PackableOverridableActorClassComponent<Propulsion>;
