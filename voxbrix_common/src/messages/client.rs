use crate::{
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
}

impl PackDefault for ClientAccept {}

#[derive(Serialize, Deserialize)]
pub struct ServerSettings {
    pub player_ticket_radius: u8,
}

impl PackDefault for ServerSettings {}
