use crate::entity::actor_model::{
    ActorModel,
    MinMax,
};
use std::collections::BTreeMap;

pub mod animation;
pub mod body_part;

pub struct ActorModelComponent<K, T> {
    data: BTreeMap<(ActorModel, K), T>,
}

impl<K, T> ActorModelComponent<K, T>
where
    K: Ord + Copy + MinMax,
{
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, model: ActorModel, key: K, new: T) -> Option<T> {
        self.data.insert((model, key), new)
    }

    pub fn get(&self, model: ActorModel, key: K) -> Option<&T> {
        self.data.get(&(model, key))
    }

    pub fn get_actor_model(
        &self,
        model: ActorModel,
    ) -> impl DoubleEndedIterator<Item = (ActorModel, K, &T)> {
        self.data
            .range((model, K::MIN) .. (model, K::MAX))
            .map(|(&(m, k), t)| (m, k, t))
    }

    pub fn extend(&mut self, iter: impl Iterator<Item = (ActorModel, K, T)>) {
        self.data.extend(iter.map(|(m, k, t)| ((m, k), t)))
    }
}
