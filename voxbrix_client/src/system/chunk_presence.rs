use crate::component::actor::position::PositionActorComponent;
use voxbrix_common::{
    component::chunk::status::StatusChunkComponent,
    entity::chunk::Chunk,
};

pub struct ChunkPresenceSystem;

impl ChunkPresenceSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn process(
        &self,
        radius: i32,
        pac: &PositionActorComponent,
        status_cc: &mut StatusChunkComponent,
        mut delete: impl FnMut(Chunk),
    ) {
        let should_exist = |chunk: &Chunk| {
            pac.player_chunks()
                .find(|ctl_chunk| ctl_chunk.radius(radius).is_within(chunk))
                .is_some()
        };

        status_cc.retain(|chunk, _| {
            let retain = should_exist(chunk);
            if !retain {
                delete(*chunk);
            }
            retain
        });
    }
}
