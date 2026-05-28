use crate::{
    math::Vec3F32,
    FromDescriptor,
};
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(PartialEq, Serialize, Deserialize, Debug)]
pub struct Sight {
    /// Offset from the actor position where sight starts, in blocks.
    pub view_offset: Vec3F32,
    /// Maximum angle from line of sight at which objects are visible, in radians.
    pub view_angle: f32,
}

impl Default for Sight {
    fn default() -> Self {
        Self {
            view_offset: Vec3F32::ZERO,
            view_angle: 0.0,
        }
    }
}

impl FromDescriptor for Sight {
    type Descriptor = Sight;

    const COMPONENT_NAME: &str = "sight";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
