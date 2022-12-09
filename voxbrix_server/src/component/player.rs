use crate::entity::player::Player;
use std::collections::BTreeMap;

pub mod actor;
pub mod client;

pub struct PlayerComponent<T> {
    data: BTreeMap<Player, T>,
}

impl<T> PlayerComponent<T> {
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn get(&self, player: &Player) -> Option<&T> {
        self.data.get(&player)
    }

    pub fn insert(&mut self, player: Player, value: T) -> Option<T> {
        self.data.insert(player, value)
    }

    pub fn remove(&mut self, player: &Player) -> Option<T> {
        self.data.remove(player)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Player, &T)> {
        self.data.iter()
    }
}
