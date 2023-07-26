#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct BlockModel(pub usize);

impl std::hash::Hash for BlockModel {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_usize(self.0)
    }
}

impl nohash_hasher::IsEnabled for BlockModel {}
