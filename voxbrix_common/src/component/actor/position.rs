use crate::{
    entity::chunk::Chunk,
    math::Vec3F32,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::ops::Add;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Position {
    pub chunk: Chunk,
    pub offset: Vec3F32,
}

#[derive(Clone, Debug)]
pub struct LocalPosition {
    pub vector: Vec3F32,
}

impl Add<LocalPosition> for LocalPosition {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        LocalPosition {
            vector: self.vector + other.vector,
        }
    }
}
