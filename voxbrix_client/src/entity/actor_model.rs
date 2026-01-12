use crate::assets::{
    ACTOR_MODEL_ANIMATION_LIST_PATH,
    ACTOR_MODEL_BONE_LIST_PATH,
};
use voxbrix_common::{
    math::MinMax,
    AsFromUsize,
    StaticEntity,
};

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorAnimation(pub u32);

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
        hasher.write_u32(self.0)
    }
}

impl MinMax for ActorAnimation {
    const MAX: Self = Self(u32::MAX);
    const MIN: Self = Self(u32::MIN);
}

impl nohash_hasher::IsEnabled for ActorAnimation {}

impl StaticEntity for ActorAnimation {
    const LIST_PATH: &str = ACTOR_MODEL_ANIMATION_LIST_PATH;
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorBone(pub u32);

impl AsFromUsize for ActorBone {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl std::hash::Hash for ActorBone {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u32(self.0)
    }
}

impl nohash_hasher::IsEnabled for ActorBone {}

impl StaticEntity for ActorBone {
    const LIST_PATH: &str = ACTOR_MODEL_BONE_LIST_PATH;
}
