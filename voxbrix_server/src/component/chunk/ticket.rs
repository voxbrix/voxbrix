use crate::component::chunk::ChunkComponent;

pub enum ChunkTicket {
    Active,
    Loading,
}

pub type TicketChunkComponent = ChunkComponent<ChunkTicket>;
