use crate::storage::{
    IntoDataSized,
    TypeName,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq, Clone, Copy, Debug)]
pub struct Player(pub u64);

impl std::hash::Hash for Player {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for Player {}

impl TypeName for Player {
    const NAME: &'static str = "Player";
}

impl IntoDataSized for Player {
    type Array = [u8; 8];

    fn to_bytes(&self) -> [u8; Self::SIZE] {
        self.0.to_be_bytes()
    }

    fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        Self(u64::from_be_bytes(*bytes))
    }
}
