use crate::component::block_model::BlockModelComponent;
use anyhow::Error;
use serde::Deserialize;
use voxbrix_common::FromDescriptor;
use voxbrix_world::World;

pub type CullingBlockModelComponent = BlockModelComponent<Culling>;

// How this block culls neighbors' sides
#[derive(Deserialize, Default, Debug)]
#[serde(tag = "kind")]
pub enum Culling {
    #[default]
    None,
    Full,
}

impl FromDescriptor for Culling {
    type Descriptor = Culling;

    const COMPONENT_NAME: &str = "culling";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
