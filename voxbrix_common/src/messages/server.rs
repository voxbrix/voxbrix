use crate::{
    entity::snapshot::Snapshot,
    messages::{
        ActionsPacked,
        StatePacked,
    },
    pack::Pack,
};
use bincode::{
    BorrowDecode,
    Encode,
};

#[derive(Encode, BorrowDecode)]
pub enum ServerAccept<'a> {
    State {
        snapshot: Snapshot,
        // last server's snapshot received by this client
        last_server_snapshot: Snapshot,
        state: StatePacked<'a>,
        actions: ActionsPacked<'a>,
    },
}

impl Pack for ServerAccept<'_> {
    const DEFAULT_COMPRESSED: bool = true;
}

#[derive(Encode, BorrowDecode)]
pub enum InitRequest {
    Login,
    Register,
}

impl Pack for InitRequest {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Encode, BorrowDecode)]
pub struct LoginRequest {
    pub username: String,
    pub key_signature: [u8; 64],
}

impl Pack for LoginRequest {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Encode, BorrowDecode)]
pub struct RegisterRequest {
    pub username: String,
    pub public_key: [u8; 33],
}

impl Pack for RegisterRequest {
    const DEFAULT_COMPRESSED: bool = false;
}
