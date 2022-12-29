use crate::{
    component::{
        actor::position::{
            GlobalPosition,
            GlobalPositionActorComponent,
        },
        block::class::ClassBlockComponent,
        chunk::status::StatusChunkComponent,
    },
    entity::actor::Actor,
    scene::game::Event,
};
use local_channel::mpsc::Sender;

pub struct ChunkPresenceSystem;

impl ChunkPresenceSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn process(
        &self,
        radius: i32,
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
        let radius = player_chunk.radius(radius as i32);

        scc.retain(|chunk, _| {
            let retain = radius.is_within(chunk);
            if !retain {
                cbc.remove_chunk(chunk);
                let _ = event_tx.send(Event::DrawChunk(*chunk));
            }
            retain
        });
    }
}
