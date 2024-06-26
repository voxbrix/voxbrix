use bincode::{
    Decode,
    Encode,
};

pub const MAX_SNAPSHOT_DIFF: u64 = 300; // approx. 15 secs

/// Currently, Snapshot(0) means totally uninitialized client/server.
/// All loops should start with Snapshot(1).
#[derive(Encode, Decode, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Snapshot(pub u64);

impl std::hash::Hash for Snapshot {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for Snapshot {}

impl Snapshot {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}
