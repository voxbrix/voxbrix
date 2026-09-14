use crate::component::player::PlayerComponent;
use voxbrix_common::entity::chunk::ChunkRadius;

// List of chunk changes for the player during the interval between ticks.
pub type ChunkUpdatePlayerComponent = PlayerComponent<ChunkUpdate>;

pub struct ChunkUpdate {
    pub previous_view: Option<ChunkRadius>,
}
