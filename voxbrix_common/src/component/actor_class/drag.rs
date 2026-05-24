use crate::FromDescriptor;
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(PartialEq, Serialize, Deserialize, Default, Debug)]
#[serde(tag = "kind")]
pub enum Drag {
    #[default]
    None,
    Uniform {
        value: [f32; 3],
    },
}

impl FromDescriptor for Drag {
    type Descriptor = Drag;

    const COMPONENT_NAME: &str = "drag";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
