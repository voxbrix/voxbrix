use crate::component::block_model::BlockModelComponent;
use serde::Deserialize;

pub type CullingBlockModelComponent = BlockModelComponent<Culling>;

// How this block culls neighbors' sides
#[derive(Deserialize, Debug)]
#[serde(tag = "kind")]
pub enum Culling {
    Full,
}
