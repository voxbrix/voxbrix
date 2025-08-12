use crate::AsFromUsize;
use serde::{
    Deserialize,
    Serialize,
};

/// Network event. Currently only comes from Server to a Client.
/// Unlike Update keeps the intermediate changes, so the same Dispatch can repeat within the same
/// snapshot and the later instances of the same Dispatch will not automatically override old ones.
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Hash, Ord, Copy, Clone, Debug)]
pub struct Dispatch(pub u32);

impl nohash_hasher::IsEnabled for Dispatch {}

impl AsFromUsize for Dispatch {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
