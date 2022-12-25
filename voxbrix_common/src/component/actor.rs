use crate::{
    entity::actor::Actor,
    sparse_vec::SparseVec,
};

pub mod class;
pub mod orientation;
pub mod position;
pub mod velocity;

pub struct ActorComponent<T> {
    actors: SparseVec<T>,
}

impl<T> ActorComponent<T> {
    pub fn new() -> Self {
        Self {
            actors: SparseVec::new(),
        }
    }

    pub fn insert(&mut self, i: Actor, new: T) -> Option<T> {
        self.actors.insert(i.0, new)
    }

    pub fn get(&self, i: &Actor) -> Option<&T> {
        self.actors.get(i.0)
    }

    pub fn get_mut(&mut self, i: &Actor) -> Option<&mut T> {
        self.actors.get_mut(i.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.actors.iter().map(|(k, v)| (Actor(k), v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Actor, &mut T)> {
        self.actors.iter_mut().map(|(k, v)| (Actor(k), v))
    }

    pub fn remove(&mut self, i: &Actor) -> Option<T> {
        self.actors.remove(i.0)
    }

    pub fn push(&mut self, new: T) -> Actor {
        Actor(self.actors.push(new))
    }
}
