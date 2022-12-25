use crate::{
    component::actor::position::GlobalPosition,
    entity::{
        actor::Actor,
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
pub struct InitialData {
    pub actor: Actor,
    // pub position: GlobalPosition,
    pub player_ticket_radius: i32,
}

impl PackDefault for InitialData {}
