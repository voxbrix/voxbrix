use crate::component::{
    actor::WithUpdate,
    actor_class::PackableOverridableActorClassComponent,
};
use voxbrix_common::component::actor_class::drag::Drag;

impl WithUpdate for Drag {
    const UPDATE: &str = "actor_drag";
}

pub type DragActorClassComponent = PackableOverridableActorClassComponent<Drag>;
