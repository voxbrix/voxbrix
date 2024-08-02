use crate::AsFromUsize;
use serde::{
    Deserialize,
    Serialize,
};

/// Network-shared state component
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct StateComponent(pub u64);

impl std::hash::Hash for StateComponent {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for StateComponent {}

impl AsFromUsize for StateComponent {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
