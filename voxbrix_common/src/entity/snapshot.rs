use serde::{
    Deserialize,
    Serialize,
};

pub const MAX_SNAPSHOT_DIFF: u64 = 200; // approx. 10 secs

/// Currently, snapshot 0 means totally uninitialized client/server.
/// All loops should start with snapshot 1.
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ServerSnapshot(pub u64);

impl std::hash::Hash for ServerSnapshot {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for ServerSnapshot {}

impl ServerSnapshot {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Currently, snapshot 0 means totally uninitialized client/server.
/// All loops should start with snapshot 1.
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ClientSnapshot(pub u64);

impl std::hash::Hash for ClientSnapshot {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for ClientSnapshot {}

impl ClientSnapshot {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}
