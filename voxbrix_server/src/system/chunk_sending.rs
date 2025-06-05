use crate::{
    component::{
        actor::position::PositionActorComponent,
        chunk::cache::CacheChunkComponent,
        player::{
            actor::ActorPlayerComponent,
            chunk_update::ChunkUpdatePlayerComponent,
            chunk_view::ChunkViewPlayerComponent,
            client::{
                ClientEvent,
                ClientPlayerComponent,
                SendData,
            },
        },
    },
    entity::player::Player,
    resource::removal_queue::RemovalQueue,
    BASE_CHANNEL,
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
    chunk_view_pc: &'a ChunkViewPlayerComponent,
    chunk_update_pc: &'a mut ChunkUpdatePlayerComponent,
    client_pc: &'a ClientPlayerComponent,
    position_ac: &'a PositionActorComponent,
    cache_cc: &'a CacheChunkComponent,
    player_rq: &'a mut RemovalQueue<Player>,
}

impl ChunkSendingSystemData<'_> {
    pub fn run(self) {
        for (player, client, prev_radius, curr_radius) in
            self.chunk_update_pc
                .drain()
                .filter_map(|(player, prev_view)| {
                    let actor = self.actor_pc.get(&player)?;
                    let client = self.client_pc.get(&player)?;
                    let position = self.position_ac.get(&actor)?;
                    let curr_view = self.chunk_view_pc.get(&player)?;
                    let curr_radius = position.chunk.radius(curr_view.radius);
                    let prev_radius = prev_view.previous_view.map(|v| v.chunk.radius(v.radius));

                    Some((player, client, prev_radius, curr_radius))
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
                        channel: BASE_CHANNEL,
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
