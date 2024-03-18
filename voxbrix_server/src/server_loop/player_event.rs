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
use bincode::Options;
use log::debug;
use voxbrix_common::{
    messages::server::ServerAccept,
    pack,
};
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

                let place_block_action = sd.action_label_map.get("place_block").unwrap();

                // Filtering out already handled actions
                for (action, _, data) in actions.data().iter()
                    .filter(|(_, snapshot, _)| *snapshot > previous_last_client_snapshot)
                {
                    // TODO read action <-> script from file
                    if *action == place_block_action {
                        let script = sd.script_registry
                            .get_script_by_label("place_block")
                            .unwrap();


                            let script_data = ScriptSharedData {
                                block_class_label_map: &sd.block_class_label_map,
                                class_bc: &mut sd.class_bc,
                            };

                        sd.script_registry.access_instance(&script, script_data, |instance| {
                            let mut store = &mut instance.store;
                            let buffer = &mut instance.buffer;
                            let instance = instance.instance;

                            let data = (player, data);

                            pack::packer()
                                .serialize_into(&mut *buffer, &data)
                                .expect("serialization should not fail");

                            let input_len = buffer.len() as u32;

                            let get_write_buffer = instance
                                .get_typed_func::<u32, u32>(&mut store, "write_buffer")
                                .unwrap();

                            let ptr = get_write_buffer
                                .call(&mut store, input_len)
                                .expect("unable to get script input buffer");

                            let memory = instance.get_memory(&mut store, "memory").unwrap();

                            let start = ptr as usize;
                            let end = start + input_len as usize;

                            (&mut memory.data_mut(&mut store)[start .. end])
                                .copy_from_slice(buffer.as_slice());

                            let run = instance
                                .get_typed_func::<u32, ()>(&mut store, "run")
                                .unwrap();

                            run.call(&mut store, input_len)
                                .expect("unable to run script");
                        });
                    }
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
