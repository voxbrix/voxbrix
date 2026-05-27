use crate::{
    math::Vec3F32,
    FromDescriptor,
};
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use std::time::Duration;
use voxbrix_world::World;

#[derive(PartialEq, Serialize, Deserialize, Default, Debug)]
#[serde(tag = "kind")]
pub enum Drag {
    #[default]
    None,
    Uniform {
        value: f32,
    },
}

impl Drag {
    pub fn apply(&self, velocity: Vec3F32, environment_density: f32, dt: Duration) -> Vec3F32 {
        let Drag::Uniform { value } = self else {
            return velocity;
        };

        let speed = velocity.length();
        let drag = value.max(0.0) * environment_density.max(0.0) * speed * speed;

        velocity - velocity.normalize_or_zero() * drag * dt.as_secs_f32()
    }
}

impl FromDescriptor for Drag {
    type Descriptor = Drag;

    const COMPONENT_NAME: &str = "drag";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
