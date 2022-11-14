use crate::component::chunk::ChunkComponent;

pub enum ChunkStatus {
    Active,
    Loading,
}

pub type StatusChunkComponent = ChunkComponent<ChunkStatus>;
