use crate::component::player::PlayerComponent;
use local_channel::mpsc::Sender;
use std::rc::Rc;
use voxbrix_common::entity::{
    actor::Actor,
    chunk::Chunk,
    snapshot::Snapshot,
};
use voxbrix_protocol::Channel;

pub type ClientPlayerComponent = PlayerComponent<Client>;

pub enum SendData {
    Owned(Vec<u8>),
    Ref(Rc<Vec<u8>>),
}

impl SendData {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(v) => v.as_slice(),
            Self::Ref(v) => v.as_slice(),
        }
    }
}

// Client loop input
pub enum ClientEvent {
    AssignActor { actor: Actor },
    SendDataUnreliable { channel: Channel, data: SendData },
    SendDataReliable { channel: Channel, data: SendData },
}

pub struct Client {
    pub tx: Sender<ClientEvent>,
    // The last server snapshot received by the client
    pub last_server_snapshot: Snapshot,
    // The last client snapshot received from the client
    pub last_client_snapshot: Snapshot,
    pub last_confirmed_chunk: Option<Chunk>,
}
