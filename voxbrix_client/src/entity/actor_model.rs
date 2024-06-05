use voxbrix_common::{
    math::MinMax,
    AsFromUsize,
};

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorAnimation(pub u64);

impl AsFromUsize for ActorAnimation {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl std::hash::Hash for ActorAnimation {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl MinMax for ActorAnimation {
    const MAX: Self = Self(u64::MAX);
    const MIN: Self = Self(u64::MIN);
}

impl nohash_hasher::IsEnabled for ActorAnimation {}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorBodyPart(pub u64);

impl AsFromUsize for ActorBodyPart {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl std::hash::Hash for ActorBodyPart {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for ActorBodyPart {}
