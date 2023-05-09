use crate::math::{
    Quat,
    Vec3,
};

#[derive(Clone, Debug)]
pub struct Orientation {
    rotation: Quat,
}

impl Orientation {
    pub fn forward(&self) -> Vec3<f32> {
        self.rotation * Vec3::FORWARD
    }

    pub fn right(&self) -> Vec3<f32> {
        self.rotation * Vec3::RIGHT
    }

    pub fn up(&self) -> Vec3<f32> {
        self.rotation * Vec3::UP
    }

    pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> Self {
        Self {
            rotation: Quat::from_axis_angle(Vec3::UP, yaw)
                * Quat::from_axis_angle(Vec3::LEFT, pitch),
        }
    }
}
