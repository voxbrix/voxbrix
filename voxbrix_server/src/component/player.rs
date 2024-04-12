use crate::entity::player::Player;
use nohash_hasher::IntMap;

pub mod actor;
pub mod chunk_update;
pub mod chunk_view;
pub mod client;

pub struct PlayerComponent<T> {
    data: IntMap<Player, T>,
}

impl<T> PlayerComponent<T> {
    pub fn new() -> Self {
        Self {
            data: IntMap::default(),
        }
    }

    pub fn get(&self, player: &Player) -> Option<&T> {
        self.data.get(player)
    }

    pub fn get_mut(&mut self, player: &Player) -> Option<&mut T> {
        self.data.get_mut(player)
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

    pub fn drain(&mut self) -> impl Iterator<Item = (Player, T)> + '_ {
        self.data.drain()
    }
}
