use voxbrix_common::math::MinMax;

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorAnimation(pub u64);

impl std::hash::Hash for ActorAnimation {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl ActorAnimation {
    pub fn from_usize(value: usize) -> Self {
        Self(value.try_into().expect("value is out of bounds"))
    }

    pub fn into_usize(self) -> usize {
        self.0.try_into().expect("value is out of bounds")
    }
}

impl MinMax for ActorAnimation {
    const MAX: Self = Self(u64::MAX);
    const MIN: Self = Self(u64::MIN);
}

impl nohash_hasher::IsEnabled for ActorAnimation {}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorBodyPart(pub u64);

impl std::hash::Hash for ActorBodyPart {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.0)
    }
}

impl nohash_hasher::IsEnabled for ActorBodyPart {}

impl ActorBodyPart {
    pub fn from_usize(value: usize) -> Self {
        Self(value.try_into().expect("value is out of bounds"))
    }

    pub fn into_usize(self) -> usize {
        self.0.try_into().expect("value is out of bounds")
    }
}
