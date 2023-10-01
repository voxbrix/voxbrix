use crate::math::MinMax;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Actor(pub u64);

impl std::hash::Hash for Actor {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for Actor {}

impl MinMax for Actor {
    const MAX: Self = Actor(u64::MAX);
    const MIN: Self = Actor(u64::MIN);
}

impl Actor {
    pub fn from_usize(value: usize) -> Self {
        Self(value.try_into().expect("value is out of bounds"))
    }

    pub fn into_usize(self) -> usize {
        self.0.try_into().expect("value is out of bounds")
    }
}
