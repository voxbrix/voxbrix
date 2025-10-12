use crate::{
    component::{
        actor::{
            chunk_activation::ChunkActivationActorComponent,
            position::PositionActorComponent,
        },
        block::class::ClassBlockComponent,
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
    resource::removal_queue::RemovalQueue,
    ChunkData,
};
use voxbrix_world::{
    System,
    SystemData,
};

#[derive(SystemData)]
pub struct ChunkAddSystemData<'a> {
    chunk_activation_ac: &'a ChunkActivationActorComponent,
    position_ac: &'a PositionActorComponent,
    status_cc: &'a mut StatusChunkComponent,

    actor_pc: &'a ActorPlayerComponent,
    client_pc: &'a ClientPlayerComponent,

    class_bc: &'a mut ClassBlockComponent,
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
        self.cache_cc
            .insert(chunk_data.chunk, data_encoded.clone().into());

        let chunk = chunk_data.chunk;

        for (player, client) in self.actor_pc.iter().filter_map(|(player, actor)| {
            let position = self.position_ac.get(actor)?;
            let chunk_ticket = self.chunk_activation_ac.get(actor)?;

            if position.chunk.radius(chunk_ticket.radius).is_within(&chunk) {
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
