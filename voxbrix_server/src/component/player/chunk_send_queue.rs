use crate::component::player::PlayerComponent;
use ahash::AHashSet;
use voxbrix_common::entity::chunk::Chunk;

// List of chunk changes for the player during interval between `World::process()` calls.
pub type ChunkSendQueuePlayerComponent = PlayerComponent<ChunkSendQueue>;

pub struct ChunkSendQueue(AHashSet<Chunk>);

impl ChunkSendQueue {
    pub fn new() -> Self {
        Self(AHashSet::new())
    }

    pub fn add(&mut self, chunk: Chunk) {
        self.0.insert(chunk);
    }

    pub fn contains(&self, chunk: &Chunk) -> bool {
        self.0.contains(chunk)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Chunk> + '_ {
        self.0.iter()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
