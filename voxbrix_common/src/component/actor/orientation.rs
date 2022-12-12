use crate::{
    component::actor::ActorComponent,
    math::{
        Quat,
        Vec3,
    },
};

pub const FORWARD: Vec3<f32> = Vec3::new([1.0, 0.0, 0.0]);
pub const BACK: Vec3<f32> = Vec3::new([-1.0, 0.0, 0.0]);
pub const RIGHT: Vec3<f32> = Vec3::new([0.0, 1.0, 0.0]);
pub const LEFT: Vec3<f32> = Vec3::new([0.0, -1.0, 0.0]);
pub const UP: Vec3<f32> = Vec3::new([0.0, 0.0, 1.0]);
pub const DOWN: Vec3<f32> = Vec3::new([0.0, 0.0, -1.0]);

pub type OrientationActorComponent = ActorComponent<Orientation>;

#[derive(Clone, Debug)]
pub struct Orientation {
    rotation: Quat,
}

impl Orientation {
    pub fn forward(&self) -> Vec3<f32> {
        self.rotation * FORWARD
    }

    pub fn right(&self) -> Vec3<f32> {
        self.rotation * RIGHT
    }

    pub fn up(&self) -> Vec3<f32> {
        self.rotation * UP
    }

    pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> Self {
        Self {
            rotation: Quat::from_axis_angle(UP, yaw) * Quat::from_axis_angle(LEFT, pitch),
        }
    }
}
