use crate::FromDescriptor;
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(PartialEq, Serialize, Deserialize, Debug)]
pub struct DimensionAcceleration(pub f32);

impl Default for DimensionAcceleration {
    fn default() -> Self {
        Self(1.0)
    }
}

impl FromDescriptor for DimensionAcceleration {
    type Descriptor = DimensionAcceleration;

    const COMPONENT_NAME: &str = "dimension_acceleration";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
