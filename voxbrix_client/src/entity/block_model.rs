#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct BlockModel(pub u64);

impl std::hash::Hash for BlockModel {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for BlockModel {}

impl BlockModel {
    pub fn from_usize(value: usize) -> Self {
        Self(value.try_into().expect("value is out of bounds"))
    }

    pub fn into_usize(self) -> usize {
        self.0.try_into().expect("value is out of bounds")
    }
}
