use crate::{
    component::{
        actor::position::PositionActorComponent,
        chunk::cache::CacheChunkComponent,
        dimension_kind::player_chunk_view::PlayerChunkViewDimensionKindComponent,
        player::{
            actor::ActorPlayerComponent,
            chunk_update::ChunkUpdatePlayerComponent,
            client::{
                ClientEvent,
                ClientPlayerComponent,
                SendData,
            },
        },
    },
    entity::player::Player,
};
use voxbrix_common::resource::removal_queue::RemovalQueue;
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ChunkSendingSystem;

impl System for ChunkSendingSystem {
    type Data<'a> = ChunkSendingSystemData<'a>;
}

#[derive(SystemData)]
pub struct ChunkSendingSystemData<'a> {
    actor_pc: &'a ActorPlayerComponent,
    chunk_update_pc: &'a mut ChunkUpdatePlayerComponent,
    client_pc: &'a ClientPlayerComponent,
    position_ac: &'a PositionActorComponent,
    cache_cc: &'a CacheChunkComponent,
    player_rq: &'a mut RemovalQueue<Player>,
    player_chunk_view_dkc: &'a PlayerChunkViewDimensionKindComponent,
}

impl ChunkSendingSystemData<'_> {
    pub fn run(self) {
        for (player, client, prev_radius, curr_radius) in
            self.chunk_update_pc
                .drain()
                .filter_map(|(player, chunk_update)| {
                    let actor = self.actor_pc.get(&player)?;
                    let client = self.client_pc.get(&player)?;
                    let position = self.position_ac.get(actor)?;
                    let curr_view = self
                        .player_chunk_view_dkc
                        .get(&position.chunk.dimension.kind);
                    let curr_radius = curr_view.to_chunk_radius(&position.chunk);

                    Some((player, client, chunk_update.previous_view, curr_radius))
                })
        {
            for chunk_data in curr_radius.into_iter_expanding().filter_map(|chunk| {
                if let Some(prev_radius) = &prev_radius {
                    if prev_radius.is_within(&chunk) {
                        return None;
                    }
                }

                self.cache_cc.get(&chunk)
            }) {
                if client
                    .tx
                    .send(ClientEvent::SendDataReliable {
                        data: SendData::Arc(chunk_data.clone().into_inner()),
                    })
                    .is_err()
                {
                    self.player_rq.enqueue(player);
                }
            }
        }
    }
}
