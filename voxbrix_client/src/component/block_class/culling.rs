use crate::component::block_class::BlockClassComponent;
use serde::Deserialize;

pub type CullingBlockClassComponent = BlockClassComponent<Culling>;

// How this block culls neighbors' sides
#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Culling {
    Full,
}
