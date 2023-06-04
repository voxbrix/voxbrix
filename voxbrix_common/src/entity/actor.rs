use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Actor(pub usize);

impl std::hash::Hash for Actor {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_usize(self.0)
    }
}

impl nohash_hasher::IsEnabled for Actor {}
