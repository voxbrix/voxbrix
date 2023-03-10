use crate::{
    entity::{
        actor::Actor,
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
    },
    pack::{
        PackDefault,
        PackZipDefault,
    },
    ChunkData,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_big_array::BigArray;

#[derive(Serialize, Deserialize)]
pub struct InitResponse {
    #[serde(with = "BigArray")]
    pub public_key: [u8; 33],
    #[serde(with = "BigArray")]
    pub key_signature: [u8; 64],
}

impl PackDefault for InitResponse {}

#[derive(Serialize, Deserialize, Debug)]
pub enum LoginFailure {
    IncorrectCredentials,
    Unknown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RegisterFailure {
    UsernameTaken,
    Unknown,
}

#[derive(Serialize, Deserialize)]
pub struct InitData {
    pub actor: Actor,
    // position: GlobalPosition,
    pub player_ticket_radius: i32,
}

impl PackDefault for InitData {}

#[derive(Serialize, Deserialize)]
pub enum LoginResult {
    Success(InitData),
    Failure(LoginFailure),
}

impl PackDefault for LoginResult {}

#[derive(Serialize, Deserialize)]
pub enum RegisterResult {
    Success(InitData),
    Failure(RegisterFailure),
}

impl PackDefault for RegisterResult {}

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
