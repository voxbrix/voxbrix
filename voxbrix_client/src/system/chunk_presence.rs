use crate::component::actor::position::PositionActorComponent;
use voxbrix_common::{
    component::{
        actor::position::Position,
        block::class::ClassBlockComponent,
        chunk::status::StatusChunkComponent,
    },
    entity::{
        actor::Actor,
        chunk::Chunk,
    },
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
        status_cc: &mut StatusChunkComponent,
        mut delete: impl FnMut(Chunk),
    ) {
        let Position {
            chunk: player_chunk,
            offset: _,
        } = gpc.get(player).unwrap();
        let radius = player_chunk.radius(radius);

        status_cc.retain(|chunk, _| {
            let retain = radius.is_within(chunk);
            if !retain {
                delete(*chunk);
            }
            retain
        });
    }
}
