use crate::AsFromUsize;
use serde::{
    Deserialize,
    Serialize,
};

/// Event sent by an Actor.
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Hash, Ord, Copy, Clone, Debug)]
pub struct Action(pub u32);

impl nohash_hasher::IsEnabled for Action {}

impl AsFromUsize for Action {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
