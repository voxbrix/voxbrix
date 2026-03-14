use crate::{
    component::{
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
            client::{
                ClientEvent,
                ClientPlayerComponent,
                SendData,
            },
        },
    },
    entity::player::Player,
};
use std::sync::Arc;
use voxbrix_common::{
    component::dimension_kind::player_chunk_view::PlayerChunkViewDimensionKindComponent,
    resource::removal_queue::RemovalQueue,
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
    client_pc: &'a ClientPlayerComponent,

    class_bc: &'a mut ClassBlockComponent,
    environment_bc: &'a mut EnvironmentBlockComponent,
    metadata_bc: &'a mut MetadataBlockComponent,
    cache_cc: &'a mut CacheChunkComponent,

    player_rq: &'a mut RemovalQueue<Player>,
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

        self.cache_cc
            .insert(chunk_data.chunk, data_encoded.clone().into());

        let chunk = chunk_data.chunk;

        for (player, client) in self.actor_pc.iter().filter_map(|(player, actor)| {
            let position = self.position_ac.get(actor)?;
            let radius = self
                .player_chunk_view_dkc
                .get(&position.chunk.dimension.kind)
                .to_chunk_radius(&position.chunk);

            if radius.is_within(&chunk) {
                Some((player, self.client_pc.get(player)?))
            } else {
                None
            }
        }) {
            if client
                .tx
                .send(ClientEvent::SendDataReliable {
                    data: SendData::Arc(data_encoded.clone()),
                })
                .is_err()
            {
                self.player_rq.enqueue(*player);
            }
        }
    }
}
