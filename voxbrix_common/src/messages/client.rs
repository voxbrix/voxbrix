use crate::{
    entity::{
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
    },
    pack::PackDefault,
    ChunkData,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub enum ClientAccept {
    ChunkData(ChunkData),
    AlterBlock {
        chunk: Chunk,
        block: Block,
        block_class: BlockClass,
    },
}

impl PackDefault for ClientAccept {}

#[derive(Serialize, Deserialize)]
pub struct ServerSettings {
    pub player_ticket_radius: u8,
}

impl PackDefault for ServerSettings {}
