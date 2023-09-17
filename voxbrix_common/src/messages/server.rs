use crate::{
    entity::{
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
        snapshot::Snapshot,
    },
    messages::{
        State,
        StatePacker,
    },
    pack::{
        Pack,
        Packer,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_big_array::BigArray;

#[derive(Serialize, Deserialize)]
pub enum ServerAccept<'a> {
    State {
        snapshot: Snapshot,
        // last server's snapshot received by this client
        last_server_snapshot: Snapshot,
        #[serde(borrow)]
        state: State<'a>,
    },
    AlterBlock {
        chunk: Chunk,
        block: Block,
        block_class: BlockClass,
    },
}

impl<'a> ServerAccept<'a> {
    pub fn pack_state(
        snapshot: Snapshot,
        last_server_snapshot: Snapshot,
        state: &mut StatePacker,
        packer: &mut Packer,
    ) -> Vec<u8> {
        let mut packed = Vec::new();

        state.pack_state(|state| {
            let msg = ServerAccept::State {
                snapshot,
                last_server_snapshot,
                state,
            };

            packer.pack(&msg, &mut packed);

            match msg {
                ServerAccept::State { state, .. } => state,
                _ => panic!(),
            }
        });

        packed
    }
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
    #[serde(with = "BigArray")]
    pub key_signature: [u8; 64],
}

impl Pack for LoginRequest {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    #[serde(with = "BigArray")]
    pub public_key: [u8; 33],
}

impl Pack for RegisterRequest {
    const DEFAULT_COMPRESSED: bool = false;
}
