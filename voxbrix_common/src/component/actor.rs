use crate::{
    entity::actor::Actor,
    sparse_vec::SparseVec,
};
use std::collections::BTreeMap;

pub mod orientation;
pub mod position;
pub mod velocity;

// FIXME after
// https://github.com/rust-lang/rust/issues/91611
// pub trait ActorComponent<T> {
//
// fn new() -> Self;
//
// fn insert(&mut self, i: Actor, new: T) -> Option<T>;
//
// fn get(&self, i: &Actor) -> Option<&T>;
//
// fn get_mut(&mut self, i: &Actor) -> Option<&mut T>;
//
// fn iter(&self) -> impl Iterator<Item = (Actor, &T)>;
//
// fn iter_mut(&mut self) -> impl Iterator<Item = (Actor, &mut T)>;
//
// fn remove(&mut self, i: &Actor) -> Option<T>;
// }

pub struct ActorComponentVec<T> {
    storage: SparseVec<T>,
}

impl<T> ActorComponentVec<T> {
    pub fn create_actor(&mut self, new: T) -> Actor {
        Actor(self.storage.push(new))
    }

    pub fn new() -> Self {
        Self {
            storage: SparseVec::new(),
        }
    }

    pub fn insert(&mut self, i: Actor, new: T) -> Option<T> {
        self.storage.insert(i.0, new)
    }

    pub fn get(&self, i: &Actor) -> Option<&T> {
        self.storage.get(i.0)
    }

    pub fn get_mut(&mut self, i: &Actor) -> Option<&mut T> {
        self.storage.get_mut(i.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.storage.iter().map(|(k, v)| (Actor(k), v))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Actor, &mut T)> {
        self.storage.iter_mut().map(|(k, v)| (Actor(k), v))
    }

    pub fn remove(&mut self, i: &Actor) -> Option<T> {
        self.storage.remove(i.0)
    }
}

pub struct ActorComponentMap<T> {
    storage: BTreeMap<Actor, T>,
}

impl<T> ActorComponentMap<T> {
    pub fn new() -> Self {
        Self {
            storage: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, i: Actor, new: T) -> Option<T> {
        self.storage.insert(i, new)
    }

    pub fn get(&self, i: &Actor) -> Option<&T> {
        self.storage.get(i)
    }

    pub fn get_mut(&mut self, i: &Actor) -> Option<&mut T> {
        self.storage.get_mut(i)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.storage.iter().map(|(&a, t)| (a, t))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Actor, &mut T)> {
        self.storage.iter_mut().map(|(&a, t)| (a, t))
    }

    pub fn remove(&mut self, i: &Actor) -> Option<T> {
        self.storage.remove(i)
    }
}
