use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::component::actor_class::sight::Sight;

pub type SightActorClassComponent = OverridableActorClassComponent<Sight>;

impl OverridableFromDescriptor for Sight {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_sight";
}
