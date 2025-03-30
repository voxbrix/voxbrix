use crate::{
    math::MinMax,
    AsFromUsize,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug)]
pub struct Effect(pub u32);

impl nohash_hasher::IsEnabled for Effect {}

impl MinMax for Effect {
    const MAX: Self = Effect(u32::MAX);
    const MIN: Self = Effect(u32::MIN);
}

impl AsFromUsize for Effect {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
