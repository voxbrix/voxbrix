use crate::component::block_class::BlockClassComponent;
use serde::Deserialize;

pub type CollisionBlockClassComponent = BlockClassComponent<Collision>;

// How this block culls neighbors' sides
#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Collision {
    SolidCube,
}
