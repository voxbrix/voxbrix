use crate::entity::player::Player;
use nohash_hasher::IntMap;

pub struct ChunkView {
    pub radius: i32,
}

pub struct ChunkViewPlayerComponent {
    data: IntMap<Player, ChunkView>,
}

impl ChunkViewPlayerComponent {
    pub fn new() -> Self {
        Self {
            data: IntMap::default(),
        }
    }

    /// Runtume replacement is NOT supported and will cause panic instead.
    pub fn insert(&mut self, player: Player, value: ChunkView) {
        if self.data.insert(player, value).is_some() {
            panic!("replacement of the player chunk view is not supported");
        }
    }

    // TODO for supporting runtime replacement this must alse accept Snapshot and
    // the struct must keep the history of changes.
    // This history match (`previous_view`) should be used in distribution of actor changes.
    pub fn get(&self, player: &Player) -> Option<&ChunkView> {
        self.data.get(player)
    }

    pub fn remove(&mut self, player: &Player) {
        self.data.remove(player);
    }
}
