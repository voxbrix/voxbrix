use crate::{
    entity::{
        actor::Actor,
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
        self,
        Pack,
        Packer,
        UnpackError,
    },
    ChunkData,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_big_array::BigArray;
use std::marker::PhantomData;

#[derive(Serialize, Deserialize)]
pub struct InitResponse {
    #[serde(with = "BigArray")]
    pub public_key: [u8; 33],
    #[serde(with = "BigArray")]
    pub key_signature: [u8; 64],
}

impl Pack for InitResponse {
    const DEFAULT_COMPRESSED: bool = false;
}

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
    // position: Position,
    pub player_chunk_view_radius: i32,
}

impl Pack for InitData {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub enum LoginResult {
    Success(InitData),
    Failure(LoginFailure),
}

impl Pack for LoginResult {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub enum RegisterResult {
    Success(InitData),
    Failure(RegisterFailure),
}

impl Pack for RegisterResult {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub enum ClientAccept<'a> {
    State {
        snapshot: Snapshot,
        // last client's snapshot received by the server
        last_client_snapshot: Snapshot,
        #[serde(borrow)]
        state: State<'a>,
    },
    ChunkData(ChunkData),
    AlterBlock {
        chunk: Chunk,
        block: Block,
        block_class: BlockClass,
    },
}

impl Pack for ClientAccept<'_> {
    const DEFAULT_COMPRESSED: bool = true;
}

impl<'a> ClientAccept<'a> {
    pub fn pack_state(
        snapshot: Snapshot,
        last_client_snapshot: Snapshot,
        state: &mut StatePacker,
        packer: &mut Packer,
    ) -> Vec<u8> {
        let mut packed = Vec::new();

        state.pack_state(|state| {
            let msg = ClientAccept::State {
                snapshot,
                last_client_snapshot,
                state,
            };

            packer.pack(&msg, &mut packed);

            match msg {
                ClientAccept::State { state, .. } => state,
                _ => panic!(),
            }
        });

        packed
    }
}

pub struct ServerActorComponentUnpacker<T> {
    data: PhantomData<T>,
}

impl<'a, T> ServerActorComponentUnpacker<T>
where
    T: Deserialize<'a>,
{
    /// None means the component was removed for the actor
    pub fn unpack(
        bytes: &'a [u8],
    ) -> Result<impl ExactSizeIterator<Item = (Actor, Option<T>)>, UnpackError> {
        pack::deserialize_from::<Vec<(Actor, Option<T>)>>(bytes)
            .map(|vec| vec.into_iter())
            .ok_or(UnpackError)
    }
}
