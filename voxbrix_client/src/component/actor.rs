use std::collections::BTreeMap;
use voxbrix_common::{
    entity::actor::Actor,
    math::MinMax,
};

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

    pub fn insert(&mut self, actor: Actor, key: K, new: T) -> Option<T> {
        self.data.insert((actor, key), new)
    }

    pub fn get(&self, actor: Actor, key: K) -> Option<&T> {
        self.data.get(&(actor, key))
    }

    pub fn get_actor(&self, actor: Actor) -> impl DoubleEndedIterator<Item = (Actor, K, &T)> {
        self.data
            .range((actor, K::MIN) .. (actor, K::MAX))
            .map(|(&(m, k), t)| (m, k, t))
    }

    pub fn extend(&mut self, iter: impl Iterator<Item = (Actor, K, T)>) {
        self.data.extend(iter.map(|(m, k, t)| ((m, k), t)))
    }

    pub fn remove(&mut self, actor: Actor, key: K) -> Option<T> {
        self.data.remove(&(actor, key))
    }
}
