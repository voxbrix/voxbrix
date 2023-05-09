use crate::{
    entity::chunk::Chunk,
    math::Vec3,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::ops::Add;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    pub chunk: Chunk,
    pub offset: Vec3<f32>,
}

#[derive(Clone, Debug)]
pub struct LocalPosition {
    pub vector: Vec3<f32>,
}

impl Add<LocalPosition> for LocalPosition {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        LocalPosition {
            vector: self.vector + other.vector,
        }
    }
}
