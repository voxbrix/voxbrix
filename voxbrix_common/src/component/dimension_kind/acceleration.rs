use crate::{
    component::{
        actor::velocity::Velocity,
        dimension_kind::DimensionKindComponent,
    },
    math::Vec3F32,
    FromDescriptor,
};
use anyhow::Error;
use serde::Deserialize;
use std::time::Duration;
use voxbrix_world::World;

#[derive(Deserialize, PartialEq, Debug, Default)]
pub struct Acceleration(Vec3F32);

impl Acceleration {
    pub fn into_velocity(&self, dt_secs: Duration) -> Velocity {
        Velocity {
            vector: self.0 * dt_secs.as_secs_f32(),
        }
    }
}

impl FromDescriptor for Acceleration {
    type Descriptor = Acceleration;

    const COMPONENT_NAME: &str = "acceleration";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}

pub type AccelerationDimensionKindComponent = DimensionKindComponent<Acceleration>;
