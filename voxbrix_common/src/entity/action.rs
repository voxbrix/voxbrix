use crate::AsFromUsize;
use bincode::{
    Decode,
    Encode,
};

#[derive(Encode, Decode, PartialEq, Eq, PartialOrd, Hash, Ord, Copy, Clone, Debug)]
pub struct Action(pub u64);

impl nohash_hasher::IsEnabled for Action {}

impl AsFromUsize for Action {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
