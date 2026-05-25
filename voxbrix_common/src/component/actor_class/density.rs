use crate::FromDescriptor;
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(PartialEq, Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Density(pub f32);

impl Default for Density {
    fn default() -> Self {
        Self(1.0)
    }
}

impl FromDescriptor for Density {
    type Descriptor = Density;

    const COMPONENT_NAME: &str = "density";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
