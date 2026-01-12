use crate::{
    component::block_class::BlockClassComponent,
    FromDescriptor,
};
use anyhow::Error;
use serde::Deserialize;
use voxbrix_world::World;

pub type CollisionBlockClassComponent = BlockClassComponent<Collision>;

// How this block culls neighbors' sides
#[derive(Deserialize, Default, Debug)]
#[serde(tag = "kind")]
pub enum Collision {
    #[default]
    None,
    SolidCube,
}

impl FromDescriptor for Collision {
    type Descriptor = Collision;

    const COMPONENT_NAME: &str = "collision";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
