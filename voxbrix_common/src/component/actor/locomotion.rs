use crate::{
    component::actor_class::propulsion::PropulsionType,
    math::Vec3F32,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Locomotion {
    pub propulsion: PropulsionType,
    /// Direction relative to actor orientation: x - forward, y - right, z - up.
    pub direction: Vec3F32,
}

impl Default for Locomotion {
    fn default() -> Self {
        Self {
            propulsion: PropulsionType::None,
            direction: Vec3F32::ZERO,
        }
    }
}

impl Locomotion {
    pub fn is_active(&self) -> bool {
        self.propulsion != PropulsionType::None && self.direction.length_squared() > f32::EPSILON
    }
}
