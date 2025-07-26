use voxbrix_common::AsFromUsize;

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Texture(u32);

impl Texture {
    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

impl AsFromUsize for Texture {
    fn as_usize(&self) -> usize {
        self.0 as usize
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().expect("texture index is too large"))
    }
}

impl nohash_hasher::IsEnabled for Texture {}
