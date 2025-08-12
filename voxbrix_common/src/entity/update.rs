use crate::AsFromUsize;
use serde::{
    Deserialize,
    Serialize,
};

/// Update of a component. Unlike Dispatch, same Update overrides content of the previous one.
/// This is meant for the components that do not need intermediate states beside the latest
/// version.
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Update(pub u32);

impl std::hash::Hash for Update {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u32(self.0)
    }
}

impl nohash_hasher::IsEnabled for Update {}

impl AsFromUsize for Update {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
