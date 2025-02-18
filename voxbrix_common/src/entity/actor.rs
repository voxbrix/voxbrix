use crate::math::MinMax;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug)]
pub struct Actor(pub u32);

impl nohash_hasher::IsEnabled for Actor {}

impl MinMax for Actor {
    const MAX: Self = Actor(u32::MAX);
    const MIN: Self = Actor(u32::MIN);
}

impl Actor {
    pub fn from_usize(i: usize) -> Actor {
        Self(i.try_into().unwrap())
    }

    pub fn into_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }
}
