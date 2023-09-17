use crate::math::{
    Directions,
    QuatF32,
    Vec3F32,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Orientation {
    rotation: QuatF32,
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

    pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> Self {
        Self {
            rotation: QuatF32::from_axis_angle(Vec3F32::UP, yaw)
                * QuatF32::from_axis_angle(Vec3F32::LEFT, pitch),
        }
    }
}
