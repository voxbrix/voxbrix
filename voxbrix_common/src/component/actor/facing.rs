use crate::{
    component::actor::ActorComponent,
    math::Vec3,
};

pub type FacingActorComponent = ActorComponent<Facing>;

#[derive(Clone, Debug)]
pub struct Facing {
    pub yaw: f32,
    pub pitch: f32,
}

impl Facing {
    pub fn forward_right(&self) -> Option<(Vec3<f32>, Vec3<f32>)> {
        let (yaw_sin, yaw_cos) = self.yaw.sin_cos();
        Some((
            Vec3::new([yaw_cos, yaw_sin, 0.0]).normalize()?,
            Vec3::new([-yaw_sin, yaw_cos, 0.0]).normalize()?,
        ))
    }

    pub fn vector(&self) -> Option<Vec3<f32>> {
        let (yaw_sin, yaw_cos) = self.yaw.sin_cos();
        let (pitch_sin, pitch_cos) = self.pitch.sin_cos();
        Vec3::new([pitch_cos * yaw_cos, pitch_cos * yaw_sin, pitch_sin]).normalize()
    }
}
