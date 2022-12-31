use crate::{
    entity::{
        actor::Actor,
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
    },
    pack::PackZipDefault,
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

impl PackZipDefault for ClientAccept {}

#[derive(Serialize, Deserialize, Debug)]
pub enum InitFailure {
    IncorrectPassword,
    Unknown,
}

#[derive(Serialize, Deserialize)]
pub enum InitResponse {
    Success {
        actor: Actor,
        // position: GlobalPosition,
        player_ticket_radius: i32,
    },
    Failure(InitFailure),
}

impl PackZipDefault for InitResponse {}
