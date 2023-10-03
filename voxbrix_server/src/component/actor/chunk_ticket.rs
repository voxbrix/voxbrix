use crate::component::actor::ActorComponent;

pub struct ActorChunkTicket {
    pub radius: i32,
}

pub type ChunkTicketActorComponent = ActorComponent<ActorChunkTicket>;
