use crate::{
    component::{
        actor::position::PositionActorComponent,
        chunk::cache::CacheChunkComponent,
        player::{
            actor::ActorPlayerComponent,
            chunk_send_queue::ChunkSendQueuePlayerComponent,
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
use rayon::prelude::*;
use std::sync::Arc;
use voxbrix_common::{
    component::dimension_kind::player_chunk_view::PlayerChunkViewDimensionKindComponent,
    entity::chunk::Chunk,
    messages::client::ClientAccept,
    pack::Packer,
    resource::removal_queue::RemovalQueue,
};
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
    chunk_send_queue_pc: &'a mut ChunkSendQueuePlayerComponent,
    position_ac: &'a PositionActorComponent,
    cache_cc: &'a CacheChunkComponent,
    player_rq: &'a RemovalQueue<Player>,
    player_chunk_view_dkc: &'a PlayerChunkViewDimensionKindComponent,
}

struct PlayerSendResult {
    player: Player,
    had_update: bool,
    had_queue: bool,
}

impl ChunkSendingSystemData<'_> {
    fn send_chunks(&self, player: Player, chunk_iter: impl Iterator<Item = Chunk>) {
        let Some(client) = self.client_pc.get(&player) else {
            return;
        };

        let complete_chunk_data = chunk_iter
            .flat_map(|chunk| self.cache_cc.get(&chunk))
            .map(|c| c.clone().into_inner())
            .collect::<Vec<Arc<[u8]>>>();

        if complete_chunk_data.is_empty() {
            return;
        }

        let mut packer = Packer::new();
        let chunk_data_bytes = packer.pack_uncompressed_to_vec(&complete_chunk_data);
        let data = packer.pack_to_vec(&ClientAccept::ChunkData(&chunk_data_bytes));

        if client
            .tx
            .send(ClientEvent::SendDataReliable {
                data: SendData::Owned(data),
            })
            .is_err()
        {
            self.player_rq.enqueue(player);
        }
    }

    pub fn run(self) {
        let players = self
            .chunk_send_queue_pc
            .iter()
            .map(|(player, _)| *player)
            .chain(
                self.chunk_update_pc
                    .iter()
                    .map(|(player, _)| *player)
                    .filter(|player| self.chunk_send_queue_pc.get(&player).is_none()),
            )
            .collect::<Vec<_>>();

        let results: Vec<PlayerSendResult> = players
            .par_iter()
            .filter_map(|&player| {
                let chunk_update = self.chunk_update_pc.get(&player);
                let chunk_queue = self.chunk_send_queue_pc.get(&player);

                let had_update = chunk_update.is_some();
                let had_queue = chunk_queue.is_some();

                let update_iter = chunk_update
                    .and_then(|chunk_update| {
                        let actor = self.actor_pc.get(&player)?;
                        let position = self.position_ac.get(actor)?;
                        let curr_view = self
                            .player_chunk_view_dkc
                            .get(&position.chunk.dimension.kind);
                        let curr_radius = curr_view.to_chunk_radius(&position.chunk);
                        let prev_radius = chunk_update.previous_view;

                        Some(
                            curr_radius
                                .into_iter_expanding()
                                .filter(move |chunk| {
                                    chunk_queue.map(|q| !q.contains(chunk)).unwrap_or(true)
                                })
                                .filter(move |chunk| {
                                    prev_radius.map(|r| !r.is_within(chunk)).unwrap_or(true)
                                }),
                        )
                    })
                    .into_iter()
                    .flatten();

                let queue_iter = chunk_queue.into_iter().flat_map(|q| q.iter().copied());

                let chunk_iter = update_iter.chain(queue_iter);

                self.send_chunks(player, chunk_iter);

                Some(PlayerSendResult {
                    player,
                    had_update,
                    had_queue,
                })
            })
            .collect();

        for result in results {
            if result.had_update {
                self.chunk_update_pc.remove(&result.player);
            }
            if result.had_queue {
                if let Some(queue) = self.chunk_send_queue_pc.get_mut(&result.player) {
                    queue.clear();
                }
            }
        }
    }
}
