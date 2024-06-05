use crate::AsFromUsize;
use nohash_hasher::IsEnabled;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Script(pub u64);

impl IsEnabled for Script {}

impl AsFromUsize for Script {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
