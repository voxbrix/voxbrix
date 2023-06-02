use voxbrix_common::entity::actor::Actor;
use voxbrix_common::math::MinMax;
use std::collections::BTreeMap;

pub mod animation_state;
pub mod class;
pub mod orientation;
pub mod position;
pub mod velocity;

pub struct ActorSubcomponent<K, T> {
    data: BTreeMap<(Actor, K), T>,
}

impl<K, T> ActorSubcomponent<K, T>
where
    K: Ord + Copy + MinMax,
{
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, model: Actor, key: K, new: T) -> Option<T> {
        self.data.insert((model, key), new)
    }

    pub fn get(&self, model: Actor, key: K) -> Option<&T> {
        self.data.get(&(model, key))
    }

    pub fn get_actor_model(
        &self,
        model: Actor,
    ) -> impl DoubleEndedIterator<Item = (Actor, K, &T)> {
        self.data
            .range((model, K::MIN) .. (model, K::MAX))
            .map(|(&(m, k), t)| (m, k, t))
    }

    pub fn extend(&mut self, iter: impl Iterator<Item = (Actor, K, T)>) {
        self.data.extend(iter.map(|(m, k, t)| ((m, k), t)))
    }
}
