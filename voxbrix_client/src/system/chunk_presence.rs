use crate::{
    component::actor::position::PositionActorComponent,
    scene::game::Event,
};
use local_channel::mpsc::Sender;
use voxbrix_common::{
    component::{
        actor::position::Position,
        block::class::ClassBlockComponent,
        chunk::status::StatusChunkComponent,
    },
    entity::actor::Actor,
};

pub struct ChunkPresenceSystem;

impl ChunkPresenceSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn process(
        &self,
        radius: i32,
        player: &Actor,
        gpc: &PositionActorComponent,
        class_bc: &mut ClassBlockComponent,
        status_cc: &mut StatusChunkComponent,
        event_tx: &Sender<Event>,
    ) {
        let Position {
            chunk: player_chunk,
            offset: _,
        } = gpc.get(player).unwrap();
        let radius = player_chunk.radius(radius);

        status_cc.retain(|chunk, _| {
            let retain = radius.is_within(chunk);
            if !retain {
                class_bc.remove_chunk(chunk);
                let _ = event_tx.send(Event::DrawChunk(*chunk));
            }
            retain
        });
    }
}
