use crate::math::{
    Directions,
    QuatF32,
    Vec3F32,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy, Debug)]
pub struct Orientation {
    pub rotation: QuatF32,
}

impl Orientation {
    pub fn forward(&self) -> Vec3F32 {
        self.rotation * Vec3F32::FORWARD
    }

    pub fn right(&self) -> Vec3F32 {
        self.rotation * Vec3F32::RIGHT
    }

    pub fn up(&self) -> Vec3F32 {
        self.rotation * Vec3F32::UP
    }
}
