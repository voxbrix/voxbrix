use crate::component::player::PlayerComponent;
use voxbrix_common::entity::chunk::Chunk;

// List of chunk changes for the player during interval between `World::process()` calls.
pub type ChunkUpdatePlayerComponent = PlayerComponent<ChunkUpdate>;

pub struct FullChunkView {
    pub chunk: Chunk,
    pub radius: i32,
}

pub struct ChunkUpdate {
    pub previous_view: Option<FullChunkView>,
}
