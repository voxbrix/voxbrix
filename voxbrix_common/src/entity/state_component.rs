use serde::{
    Deserialize,
    Serialize,
};

/// Network-shared state component
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct StateComponent(pub u32);

impl std::hash::Hash for StateComponent {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u32(self.0)
    }
}

impl nohash_hasher::IsEnabled for StateComponent {}
