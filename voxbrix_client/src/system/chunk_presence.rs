use crate::{
    component::{
        actor::position::{
            GlobalPosition,
            GlobalPositionActorComponent,
        },
        block::class::ClassBlockComponent,
        chunk::status::StatusChunkComponent,
    },
    entity::{
        actor::Actor,
        chunk::Chunk,
    },
    event_loop::Event,
};
use local_channel::mpsc::Sender;
use voxbrix_common::messages::client::ServerSettings;

pub struct ChunkPresenceSystem;

impl ChunkPresenceSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn process(
        &self,
        settings: &ServerSettings,
        player: &Actor,
        gpc: &GlobalPositionActorComponent,
        cbc: &mut ClassBlockComponent,
        scc: &mut StatusChunkComponent,
        event_tx: &Sender<Event>,
    ) {
        let GlobalPosition {
            chunk: player_chunk,
            offset: _,
        } = gpc.get(player).unwrap();
        let radius = settings.player_ticket_radius as i32;
        let retain_fn = |chunk: &Chunk| {
            chunk.dimension == player_chunk.dimension
                && chunk.position[0] >= player_chunk.position[0].saturating_sub(radius)
                && chunk.position[0] <= player_chunk.position[0].saturating_add(radius)
                && chunk.position[1] >= player_chunk.position[1].saturating_sub(radius)
                && chunk.position[1] <= player_chunk.position[1].saturating_add(radius)
                && chunk.position[2] >= player_chunk.position[2].saturating_sub(radius)
                && chunk.position[2] <= player_chunk.position[2].saturating_add(radius)
        };

        scc.retain(|chunk, _| {
            let retain = retain_fn(chunk);
            if !retain {
                cbc.remove_chunk(chunk);
                let _ = event_tx.send(Event::DrawChunk { chunk: *chunk });
            }
            retain
        });
    }
}
