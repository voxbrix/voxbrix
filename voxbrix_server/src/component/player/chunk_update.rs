use crate::component::player::PlayerComponent;
use voxbrix_common::entity::chunk::ChunkRadius;

// List of chunk changes for the player during interval between `World::process()` calls.
pub type ChunkUpdatePlayerComponent = PlayerComponent<ChunkUpdate>;

pub struct ChunkUpdate {
    pub previous_ticket: Option<ChunkRadius>,
}
