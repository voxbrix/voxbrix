use crate::FromDescriptor;
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(PartialEq, Serialize, Deserialize, Debug)]
pub struct GravitySensitivity(pub f32);

impl Default for GravitySensitivity {
    fn default() -> Self {
        Self(1.0)
    }
}

impl FromDescriptor for GravitySensitivity {
    type Descriptor = GravitySensitivity;

    const COMPONENT_NAME: &str = "gravity_sensitivity";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
