use crate::{
    component::actor::{
        facing::FacingActorComponent,
        position::PositionActorComponent,
    },
    entity::actor::Actor,
    linear_algebra::{
        Mat4,
        Vec3,
    },
};

const UP_VECTOR: Vec3<f32> = Vec3::new(0.0, 0.0, 1.0);

#[derive(Debug)]
pub struct Camera {
    pub actor: Actor,
}

impl Camera {
    pub fn new(actor: Actor) -> Self {
        Self { actor }
    }

    pub fn calc_matrix(
        &self,
        position: &PositionActorComponent,
        facing: &FacingActorComponent,
    ) -> Mat4<f32> {
        let actor_facing = facing.get(self.actor).unwrap();
        let (pitch_sin, pitch_cos) = actor_facing.pitch.sin_cos();
        let (yaw_sin, yaw_cos) = actor_facing.yaw.sin_cos();
        nalgebra_glm::translate(
            &nalgebra_glm::quat_to_mat4(&nalgebra_glm::quat_look_at_lh(
                &Vec3::new(yaw_cos * pitch_cos, yaw_sin * pitch_cos, pitch_sin),
                &UP_VECTOR,
            )),
            &-position.get(self.actor).unwrap().vector,
        )
    }
}

pub struct Projection {
    aspect: f32,
    fovy: f32,
    znear: f32,
    zfar: f32,
}

impl Projection {
    pub fn new(width: u32, height: u32, fovy: f32, znear: f32, zfar: f32) -> Self {
        Self {
            aspect: width as f32 / height as f32,
            fovy,
            znear,
            zfar,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
    }

    pub fn calc_matrix(&self) -> Mat4<f32> {
        nalgebra_glm::perspective_lh(self.aspect, self.fovy, self.znear, self.zfar)
    }
}
