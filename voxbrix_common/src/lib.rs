pub mod component;
pub mod entity;
pub mod math;
pub mod messages;
pub mod pack;
pub mod sparse_vec;

use component::block::Blocks;
use entity::{
    block_class::BlockClass,
    chunk::Chunk,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub struct ChunkData {
    pub chunk: Chunk,
    pub block_classes: Blocks<BlockClass>,
}
