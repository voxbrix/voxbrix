use crate::FromDescriptor;
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(PartialEq, Serialize, Deserialize, Default, Debug)]
#[serde(tag = "kind")]
pub enum BlockCollision {
    #[default]
    None,
    AABB {
        radius_blocks: [f32; 3],
    },
}

impl FromDescriptor for BlockCollision {
    type Descriptor = BlockCollision;

    const COMPONENT_NAME: &str = "block_collision";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
