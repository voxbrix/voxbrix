use crate::FromDescriptor;
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct Health {
    current: u32,
    maximum: u32,
}

impl Health {
    pub fn full(maximum: u32) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }

    pub fn current(&self) -> u32 {
        self.current
    }

    pub fn ratio(&self) -> Option<f32> {
        let ratio = self.current as f32 / self.maximum as f32;

        if ratio.is_nan() {
            return None;
        }

        Some(ratio.min(1.0))
    }

    pub fn damage(&mut self, amount: u32) {
        self.current = self.current.saturating_sub(amount);
    }
}

impl FromDescriptor for Health {
    type Descriptor = Health;

    const COMPONENT_NAME: &str = "health";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        desc.ok_or_else(|| Error::msg("health descriptor is missing"))
    }
}
