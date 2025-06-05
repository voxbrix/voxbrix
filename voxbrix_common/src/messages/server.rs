use crate::{
    entity::snapshot::Snapshot,
    messages::{
        ActionsPacked,
        StatePacked,
    },
    pack::Pack,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub struct ClientState<'a> {
    pub snapshot: Snapshot,
    // last server's snapshot received by this client
    pub last_server_snapshot: Snapshot,
    #[serde(borrow)]
    pub state: StatePacked<'a>,
    #[serde(borrow)]
    pub actions: ActionsPacked<'a>,
}

#[derive(Serialize, Deserialize)]
pub enum ServerAccept<'a> {
    State(#[serde(borrow)] ClientState<'a>),
}

impl Pack for ServerAccept<'_> {
    const DEFAULT_COMPRESSED: bool = true;
}

#[derive(Serialize, Deserialize)]
pub enum InitRequest {
    Login,
    Register,
}

impl Pack for InitRequest {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    #[serde(with = "serde_big_array::BigArray")]
    pub key_signature: [u8; 64],
}

impl Pack for LoginRequest {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    #[serde(with = "serde_big_array::BigArray")]
    pub public_key: [u8; 33],
}

impl Pack for RegisterRequest {
    const DEFAULT_COMPRESSED: bool = false;
}
