use voxbrix_common::component::chunk::ChunkComponent;

#[derive(PartialEq, Eq, Debug)]
pub enum ChunkStatus {
    Active,
    Loading,
}

pub type StatusChunkComponent = ChunkComponent<ChunkStatus>;
