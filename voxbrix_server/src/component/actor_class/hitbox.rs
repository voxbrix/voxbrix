use crate::component::actor_class::PackableOverridableActorClassComponent;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(tag = "kind")]
pub enum Hitbox {
    Sphere { radius_blocks: f32 },
}

pub type HitboxActorClassComponent = PackableOverridableActorClassComponent<Hitbox>;
