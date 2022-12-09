use crate::{
    component::actor::ActorComponent,
    entity::chunk::Chunk,
};

pub struct ActorChunkTicket {
    pub chunk: Chunk,
    pub radius: i32,
}

pub type ChunkTicketActorComponent = ActorComponent<ActorChunkTicket>;
