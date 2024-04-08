use crate::{
    component::player::chunk_update::{
        ChunkUpdate,
        FullChunkView,
    },
    entity::player::Player,
    server_loop::data::{
        ScriptSharedData,
        SharedData,
    },
};
use log::{
    debug,
    warn,
};
use voxbrix_common::messages::server::ServerAccept;
use voxbrix_protocol::server::Packet;

pub struct PlayerEvent<'a> {
    pub shared_data: &'a mut SharedData,
    pub player: Player,
    pub data: Packet,
}

impl PlayerEvent<'_> {
    pub fn run(self) {
        let Self {
            shared_data: sd,
            player,
            data,
        } = self;

        let event = match sd.packer.unpack::<ServerAccept>(data.as_ref()) {
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
                actions,
            } => {
                let state = match sd.state_unpacker.unpack_state(state) {
                    Ok(v) => v,
                    Err(_) => {
                        debug!("skipping corrupted state");
                        return;
                    },
                };

                // TODO: there could be sequential messaged on the wire that have the same state
                // changes duplicated. Make sure that those do not duplicate in the components.
                let actor = match sd.actor_pc.get(&player) {
                    Some(a) => a,
                    None => return,
                };

                let client = match sd.client_pc.get_mut(&player) {
                    Some(c) => c,
                    None => return,
                };

                if client.last_client_snapshot >= last_client_snapshot {
                    return;
                }

                let previous_last_client_snapshot = client.last_client_snapshot;

                client.last_server_snapshot = last_server_snapshot;
                client.last_client_snapshot = last_client_snapshot;

                sd.velocity_ac.unpack_player(actor, &state, sd.snapshot);
                sd.orientation_ac.unpack_player(actor, &state, sd.snapshot);

                sd.position_ac.unpack_player_with(
                    actor,
                    &state,
                    sd.snapshot,
                    |old_value, new_value| {
                        let chunk = match new_value {
                            Some(v) => v.chunk,
                            None => return,
                        };

                        sd.client_pc.get_mut(&player).unwrap().last_confirmed_chunk = Some(chunk);

                        if old_value.is_none()
                            || old_value.is_some() && old_value.unwrap().chunk != chunk
                        {
                            let prev_view_radius = match sd.chunk_view_pc.get(&player) {
                                Some(r) => r.radius,
                                None => return,
                            };

                            let previous_view = old_value.map(|old_pos| {
                                FullChunkView {
                                    chunk: old_pos.chunk,
                                    radius: prev_view_radius,
                                }
                            });

                            if sd.chunk_update_pc.get(&player).is_some() {
                                return;
                            } else {
                                sd.chunk_update_pc
                                    .insert(player, ChunkUpdate { previous_view });
                            }
                        }
                    },
                );

                let actions = match sd.actions_unpacker.unpack_actions(actions) {
                    Ok(v) => v,
                    Err(_) => {
                        debug!("unable to unpack actions");
                        return;
                    }
                };

                // Filtering out already handled actions
                for (action, _, data) in actions.data().iter()
                    .filter(|(_, snapshot, _)| *snapshot > previous_last_client_snapshot)
                {
                    let Some(script) = sd.script_action_component.get(action) else {
                        warn!("script for \"{:?}\" not found", action);
                        continue;
                    };

                    let Some(actor) = sd.actor_pc.get(&player) else {
                        warn!("actor for player {:?} not found", player);
                        continue;
                    };

                    let script_data = ScriptSharedData {
                        block_class_label_map: &sd.block_class_label_map,
                        class_bc: &sd.class_bc,
                        class_change_bc: &mut sd.class_change_bc,
                        collision_bcc: &sd.collision_bcc,
                    };

                    sd.script_registry.run_script(&script, script_data, (Some(actor), data));
                }

            },
            /*ServerAccept::AlterBlock {
                chunk,
                block,
                block_class,
            } => {
                if let Some(block_class_ref) = sd
                    .class_bc
                    .get_mut_chunk(&chunk)
                    .map(|blocks| blocks.get_mut(block))
                {
                    *block_class_ref = block_class;

                    let data_buf = Arc::new(sd.packer.pack_to_vec(&ClientAccept::AlterBlock {
                        chunk,
                        block,
                        block_class,
                    }));

                    for (player, client) in sd.actor_pc.iter().filter_map(|(player, actor)| {
                        let view = sd.chunk_view_pc.get(player)?;
                        let position = sd.position_ac.get(actor)?;

                        position
                            .chunk
                            .radius(view.radius)
                            .is_within(&chunk)
                            .then_some(())?;
                        let client = sd.client_pc.get(player)?;
                        Some((player, client))
                    }) {
                        if client
                            .tx
                            .send(ClientEvent::SendDataReliable {
                                channel: BASE_CHANNEL,
                                data: SendData::Arc(data_buf.clone()),
                            })
                            .is_err()
                        {
                            sd.remove_queue.remove_player(player);
                        }
                    }

                    // TODO unify block alterations in Process tick
                    // and update cache there
                    // possibly also unblock/rayon, this takes around 1ms for existence_ach
                    // chunk
                    let blocks_cache = sd.class_bc.get_chunk(&chunk).unwrap().clone();

                    let cache_data = ClientAccept::ChunkData(ChunkData {
                        chunk,
                        block_classes: blocks_cache,
                    });

                    sd.cache_cc
                        .insert(chunk, ChunkCache::new(sd.packer.pack_to_vec(&cache_data)));

                    let blocks_cache = match cache_data {
                        ClientAccept::ChunkData(b) => b.block_classes,
                        _ => panic!(),
                    };

                    let database = sd.database.clone();

                    sd.storage.execute(move || {
                        let mut packer = Packer::new();
                        let db_write = database.begin_write().unwrap();
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
            },*/
        }
    }
}
