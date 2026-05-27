use crate::FromDescriptor;
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(PartialEq, Serialize, Deserialize, Default, Debug)]
pub struct Propulsion {
    pub ground: GroundPropulsion,
    pub buoyant: BuoyantPropulsion,
}

#[derive(PartialEq, Serialize, Deserialize, Default, Debug)]
pub struct GroundPropulsion {
    pub movement_speed: f32,
    pub jump_velocity: f32,
}

#[derive(PartialEq, Serialize, Deserialize, Default, Debug)]
pub struct BuoyantPropulsion {
    pub magnitude: f32,
    pub max_speed: f32,
    pub density_scalability: f32,
}

impl BuoyantPropulsion {
    pub fn acceleration(&self, environment_density: f32) -> f32 {
        self.magnitude * environment_density.powf(self.density_scalability)
    }
}

impl FromDescriptor for Propulsion {
    type Descriptor = Propulsion;

    const COMPONENT_NAME: &str = "propulsion";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
