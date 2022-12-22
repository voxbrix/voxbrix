use crate::{
    component::actor::position::GlobalPosition,
    entity::{
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
    },
    pack::PackDefault,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub enum ServerAccept {
    PlayerPosition {
        position: GlobalPosition,
    },
    AlterBlock {
        chunk: Chunk,
        block: Block,
        block_class: BlockClass,
    },
}

impl PackDefault for ServerAccept {}
