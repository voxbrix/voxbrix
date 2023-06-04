use voxbrix_common::math::MinMax;

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorModel(pub usize);

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorAnimation(pub usize);

impl MinMax for ActorAnimation {
    const MAX: Self = Self(usize::MAX);
    const MIN: Self = Self(usize::MIN);
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorBodyPart(pub usize);

impl std::hash::Hash for ActorBodyPart {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_usize(self.0)
    }
}

impl nohash_hasher::IsEnabled for ActorBodyPart {}

impl MinMax for ActorBodyPart {
    const MAX: Self = Self(usize::MAX);
    const MIN: Self = Self(usize::MIN);
}
