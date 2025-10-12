use crate::component::player::PlayerComponent;
use flume::Sender;
use std::sync::Arc;
use voxbrix_common::entity::{
    actor::Actor,
    chunk::Chunk,
    snapshot::{
        ClientSnapshot,
        ServerSnapshot,
    },
};

pub type ClientPlayerComponent = PlayerComponent<Client>;

pub enum SendData {
    Owned(Vec<u8>),
    Arc(Arc<[u8]>),
}

impl SendData {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(v) => v.as_slice(),
            Self::Arc(v) => v.as_ref(),
        }
    }
}

// Client loop input
pub enum ClientEvent {
    AssignActor { actor: Actor },
    SendDataUnreliable { data: SendData },
    SendDataReliable { data: SendData },
}

pub struct Client {
    pub tx: Sender<ClientEvent>,
    // The last server snapshot received by the client
    pub last_server_snapshot: ServerSnapshot,
    // The last client snapshot received from the client
    pub last_client_snapshot: ClientSnapshot,
    pub last_confirmed_chunk: Option<Chunk>,
    pub session_id: u64,
}
