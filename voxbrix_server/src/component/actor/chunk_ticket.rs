use crate::component::actor::ActorComponent;
use voxbrix_common::entity::chunk::Chunk;

pub struct ActorChunkTicket {
    pub chunk: Chunk,
    pub radius: i32,
}

pub type ChunkTicketActorComponent = ActorComponent<ActorChunkTicket>;
