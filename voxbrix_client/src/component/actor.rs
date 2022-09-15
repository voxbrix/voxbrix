use crate::entity::actor::Actor;

pub mod class;
pub mod facing;
pub mod position;
pub mod velocity;

pub struct ActorComponent<T> {
    actors: Vec<Option<T>>,
}

impl<T> ActorComponent<T> {
    pub fn new() -> Self {
        Self { actors: Vec::new() }
    }

    pub fn set(&mut self, i: Actor, new: T) {
        if self.actors.len() > i.0 {
            self.actors[i.0] = Some(new);
        } else {
            self.actors.resize_with(i.0, || None);
            self.actors.push(Some(new));
        }
    }

    pub fn get(&self, i: Actor) -> Option<&T> {
        self.actors.get(i.0)?.as_ref()
    }

    pub fn get_mut(&mut self, i: Actor) -> &mut Option<T> {
        if i.0 < self.actors.len() {
            self.actors.get_mut(i.0).unwrap()
        } else {
            self.actors.resize_with(i.0 + 1, || None);
            self.actors.get_mut(i.0).unwrap()
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.actors
            .iter()
            .enumerate()
            .filter_map(|(i, v)| Some((Actor(i), v.as_ref()?)))
    }
}
