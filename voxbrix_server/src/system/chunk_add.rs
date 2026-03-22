use crate::component::{
    actor::position::PositionActorComponent,
    block::{
        class::ClassBlockComponent,
        environment::EnvironmentBlockComponent,
        metadata::MetadataBlockComponent,
    },
    chunk::{
        cache::CacheChunkComponent,
        status::{
            ChunkStatus,
            StatusChunkComponent,
        },
    },
    player::{
        actor::ActorPlayerComponent,
        chunk_send_queue::ChunkSendQueuePlayerComponent,
    },
};
use std::sync::Arc;
use voxbrix_common::{
    component::dimension_kind::player_chunk_view::PlayerChunkViewDimensionKindComponent,
    ChunkData,
};
use voxbrix_world::{
    System,
    SystemData,
};

#[derive(SystemData)]
pub struct ChunkAddSystemData<'a> {
    position_ac: &'a PositionActorComponent,
    status_cc: &'a mut StatusChunkComponent,
    player_chunk_view_dkc: &'a PlayerChunkViewDimensionKindComponent,

    actor_pc: &'a ActorPlayerComponent,
    chunk_send_queue_pc: &'a mut ChunkSendQueuePlayerComponent,

    class_bc: &'a mut ClassBlockComponent,
    environment_bc: &'a mut EnvironmentBlockComponent,
    metadata_bc: &'a mut MetadataBlockComponent,
    cache_cc: &'a mut CacheChunkComponent,
}

pub struct ChunkAddSystem;

impl System for ChunkAddSystem {
    type Data<'a> = ChunkAddSystemData<'a>;
}

impl ChunkAddSystemData<'_> {
    pub fn run(self, chunk_data: ChunkData, data_encoded: Arc<[u8]>) {
        match self.status_cc.get_mut(&chunk_data.chunk) {
            Some(status) if *status == ChunkStatus::Loading => {
                *status = ChunkStatus::Active;
            },
            _ => return,
        }

        self.class_bc
            .insert_chunk(chunk_data.chunk, chunk_data.block_classes);
        self.environment_bc
            .insert_chunk(chunk_data.chunk, chunk_data.block_environment);
        self.metadata_bc
            .insert_chunk(chunk_data.chunk, chunk_data.block_metadata);

        self.cache_cc.insert(chunk_data.chunk, data_encoded.into());

        let chunk = chunk_data.chunk;

        for player in self.actor_pc.iter().filter_map(|(player, actor)| {
            let position = self.position_ac.get(actor)?;
            let radius = self
                .player_chunk_view_dkc
                .get(&position.chunk.dimension.kind)
                .to_chunk_radius(&position.chunk);

            radius.is_within(&chunk).then_some(player)
        }) {
            if let Some(send_queue) = self.chunk_send_queue_pc.get_mut(player) {
                send_queue.add(chunk);
            }
        }
    }
}
