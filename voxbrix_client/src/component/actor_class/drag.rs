use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::component::actor_class::drag::Drag;

pub type DragActorClassComponent = OverridableActorClassComponent<Drag>;

impl OverridableFromDescriptor for Drag {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_drag";
}
