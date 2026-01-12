use crate::{
    assets::{
        EFFECT_DIR,
        EFFECT_LIST_PATH,
    },
    math::MinMax,
    resource::component_map::ComponentMapEntity,
    AsFromUsize,
    StaticEntity,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug)]
pub struct Effect(pub u32);

impl nohash_hasher::IsEnabled for Effect {}

impl MinMax for Effect {
    const MAX: Self = Effect(u32::MAX);
    const MIN: Self = Effect(u32::MIN);
}

impl AsFromUsize for Effect {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl StaticEntity for Effect {
    const LIST_PATH: &str = EFFECT_LIST_PATH;
}

/// Parameter that differentiates instances of the same Effect on an individual Actor.
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug)]
pub struct EffectDiscriminant(pub u64);

impl EffectDiscriminant {
    /// Only use with effects that have no discriminants.
    pub fn none() -> Self {
        Self(0)
    }
}

impl nohash_hasher::IsEnabled for EffectDiscriminant {}

impl MinMax for EffectDiscriminant {
    const MAX: Self = EffectDiscriminant(u64::MAX);
    const MIN: Self = EffectDiscriminant(u64::MIN);
}

impl ComponentMapEntity for Effect {
    const COMPONENT_MAP_DIR: &str = EFFECT_DIR;
}
