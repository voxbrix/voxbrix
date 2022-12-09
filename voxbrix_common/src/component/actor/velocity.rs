use crate::{
    component::actor::{
        position::LocalPosition,
        ActorComponent,
    },
    math::Vec3,
};
use std::{
    ops::{
        Add,
        Mul,
    },
    time::Duration,
};

pub type VelocityActorComponent = ActorComponent<Velocity>;

#[derive(Clone, Debug)]
pub struct Velocity {
    pub vector: Vec3<f32>,
}

impl Add<Velocity> for Velocity {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Velocity {
            vector: self.vector + other.vector,
        }
    }
}

impl Mul<Duration> for Velocity {
    type Output = LocalPosition;

    fn mul(self, other: Duration) -> LocalPosition {
        LocalPosition {
            vector: self.vector * other.as_secs_f32(),
        }
    }
}
