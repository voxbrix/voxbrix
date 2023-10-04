use crate::{
    component::player::{
        chunk_update::{
            ChunkUpdate,
            FullChunkView,
        },
        client::{
            ClientEvent,
            SendData,
        },
    },
    entity::player::Player,
    storage::{
        IntoData,
        IntoDataSized,
    },
    world::World,
    BASE_CHANNEL,
    BLOCK_CLASS_TABLE,
};
use log::debug;
use std::rc::Rc;
use voxbrix_common::{
    messages::{
        client::ClientAccept,
        server::ServerAccept,
    },
    pack::Packer,
    ChunkData,
};
use voxbrix_protocol::server::Packet;

impl World {
    pub fn player_event(&mut self, player: Player, data: Packet) {
        let event = match self.packer.unpack::<ServerAccept>(data.as_ref()) {
            Ok(e) => e,
            Err(_) => {
                debug!(
                    "server_loop: unable to parse data from player {:?} on base channel",
                    player
                );
                return;
            },
        };

        match event {
            ServerAccept::State {
                snapshot: last_client_snapshot,
                last_server_snapshot,
                state,
            } => {
                let actor = match self.actor_pc.get(&player) {
                    Some(a) => a,
                    None => return,
                };

                let client = match self.client_pc.get_mut(&player) {
                    Some(c) => c,
                    None => return,
                };

                if client.last_client_snapshot >= last_client_snapshot {
                    return;
                }

                client.last_server_snapshot = last_server_snapshot;
                client.last_client_snapshot = last_client_snapshot;

                self.velocity_ac.unpack_player(actor, &state, self.snapshot);
                self.orientation_ac
                    .unpack_player(actor, &state, self.snapshot);

                self.position_ac.unpack_player_with(
                    actor,
                    &state,
                    self.snapshot,
                    |old_value, new_value| {
                        let chunk = match new_value {
                            Some(v) => v.chunk,
                            None => return,
                        };

                        self.client_pc
                            .get_mut(&player)
                            .unwrap()
                            .last_confirmed_chunk = Some(chunk);

                        if old_value.is_none()
                            || old_value.is_some() && old_value.unwrap().chunk != chunk
                        {
                            let prev_view_radius = match self.chunk_view_pc.get(&player) {
                                Some(r) => r.radius,
                                None => return,
                            };

                            let previous_view = old_value.map(|old_pos| {
                                FullChunkView {
                                    chunk: old_pos.chunk,
                                    radius: prev_view_radius,
                                }
                            });

                            if self.chunk_update_pc.get(&player).is_some() {
                                return;
                            } else {
                                self.chunk_update_pc
                                    .insert(player, ChunkUpdate { previous_view });
                            }
                        }
                    },
                );
            },
            ServerAccept::AlterBlock {
                chunk,
                block,
                block_class,
            } => {
                if let Some(block_class_ref) = self
                    .class_bc
                    .get_mut_chunk(&chunk)
                    .map(|blocks| blocks.get_mut(block))
                {
                    *block_class_ref = block_class;

                    let data_buf = Rc::new(self.packer.pack_to_vec(&ClientAccept::AlterBlock {
                        chunk,
                        block,
                        block_class,
                    }));

                    for (player, client) in self.actor_pc.iter().filter_map(|(player, actor)| {
                        let view = self.chunk_view_pc.get(player)?;
                        let position = self.position_ac.get(actor)?;

                        position
                            .chunk
                            .radius(view.radius)
                            .is_within(&chunk)
                            .then_some(())?;
                        let client = self.client_pc.get(player)?;
                        Some((player, client))
                    }) {
                        if client
                            .tx
                            .send(ClientEvent::SendDataReliable {
                                channel: BASE_CHANNEL,
                                data: SendData::Ref(data_buf.clone()),
                            })
                            .is_err()
                        {
                            self.remove_queue.remove_player(player);
                        }
                    }

                    // TODO unify block alterations in Process tick
                    // and update cache there
                    // possibly also unblock/rayon, this takes around 1ms for existence_ach
                    // chunk
                    let blocks_cache = self.class_bc.get_chunk(&chunk).unwrap().clone();

                    let cache_data = ClientAccept::ChunkData(ChunkData {
                        chunk,
                        block_classes: blocks_cache,
                    });

                    self.cache_cc
                        .insert(chunk, Rc::new(self.packer.pack_to_vec(&cache_data)));

                    let blocks_cache = match cache_data {
                        ClientAccept::ChunkData(b) => b.block_classes,
                        _ => panic!(),
                    };

                    let shared = self.shared;

                    self.storage.execute(move || {
                        let mut packer = Packer::new();
                        let db_write = shared.database.begin_write().unwrap();
                        {
                            let mut table = db_write.open_table(BLOCK_CLASS_TABLE).unwrap();

                            table
                                .insert(
                                    chunk.into_data_sized(),
                                    blocks_cache.into_data(&mut packer),
                                )
                                .expect("server_loop: database write");
                        }
                        db_write.commit().unwrap();
                    });
                }
            },
        }
    }
}
