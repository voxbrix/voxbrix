use crate::{
    component::actor::position::Position,
    entity::{
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
    },
    pack::{
        PackDefault,
        PackZipDefault,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_big_array::BigArray;

#[derive(Serialize, Deserialize)]
pub enum ServerAccept {
    PlayerPosition {
        position: Position,
    },
    AlterBlock {
        chunk: Chunk,
        block: Block,
        block_class: BlockClass,
    },
}

impl PackZipDefault for ServerAccept {}

#[derive(Serialize, Deserialize)]
pub enum InitRequest {
    Login,
    Register,
}

impl PackDefault for InitRequest {}

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    #[serde(with = "BigArray")]
    pub key_signature: [u8; 64],
}

impl PackDefault for LoginRequest {}

#[derive(Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    #[serde(with = "BigArray")]
    pub public_key: [u8; 33],
}

impl PackDefault for RegisterRequest {}
