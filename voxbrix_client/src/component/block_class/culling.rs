use serde::Deserialize;
use voxbrix_common::component::block_class::BlockClassComponent;

pub type CullingBlockClassComponent = BlockClassComponent<Culling>;

// How this block culls neighbors' sides
#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Culling {
    Full,
}
