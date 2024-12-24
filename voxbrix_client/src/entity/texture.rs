use voxbrix_common::AsFromUsize;

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Texture(usize);

impl AsFromUsize for Texture {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl nohash_hasher::IsEnabled for Texture {}
