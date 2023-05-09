use voxbrix_common::{
    component::actor::ActorComponentVec,
    entity::chunk::Chunk,
};

pub struct ActorChunkTicket {
    pub chunk: Chunk,
    pub radius: i32,
}

pub type ChunkTicketActorComponent = ActorComponentVec<ActorChunkTicket>;
