use voxbrix_common::AsFromUsize;

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct BlockModel(pub u64);

impl AsFromUsize for BlockModel {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl std::hash::Hash for BlockModel {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for BlockModel {}
