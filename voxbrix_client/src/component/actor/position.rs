use crate::{
    component::actor::ActorComponent,
    linear_algebra::Vec3,
};
use std::ops::Add;

pub type PositionActorComponent = ActorComponent<Position>;

#[derive(Clone, Debug)]
pub struct Position {
    pub vector: Vec3<f32>,
}

impl Add<Position> for Position {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Position {
            vector: self.vector + other.vector,
        }
    }
}
