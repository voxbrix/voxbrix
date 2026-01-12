use crate::component::actor_class::{
    PackableOverridableActorClassComponent,
    WithUpdate,
};
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_common::FromDescriptor;
use voxbrix_world::World;

#[derive(Serialize, Deserialize, PartialEq, Default, Debug)]
#[serde(tag = "kind")]
pub enum Hitbox {
    #[default]
    None,
    Sphere {
        radius_blocks: f32,
    },
}

impl FromDescriptor for Hitbox {
    type Descriptor = Hitbox;

    const COMPONENT_NAME: &str = "hitbox";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}

impl WithUpdate for Hitbox {
    const UPDATE: &str = "actor_hitbox";
}

pub type HitboxActorClassComponent = PackableOverridableActorClassComponent<Hitbox>;
