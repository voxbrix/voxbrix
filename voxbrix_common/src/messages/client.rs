use crate::ChunkData;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub enum ClientAccept {
    ChunkData(ChunkData),
}
