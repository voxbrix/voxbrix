use crate::{
    component::player::chunk_update::{
        ChunkUpdate,
        FullChunkView,
    },
    entity::player::Player,
    server_loop::data::{
        ScriptSharedData,
        SendMutPtr,
        SendPtr,
        SharedData,
    },
};
use log::{
    debug,
    warn,
};
use server_loop_api::ActionInput;
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

                // Pruning confirmed Server -> Client actions.
                sd.actions_packer_pc
                    .get_mut(&player)
                    .expect("actions packer not found for a player")
                    .confirm_snapshot(last_server_snapshot);

                let actions = match sd.actions_unpacker.unpack_actions(actions) {
                    Ok(v) => v,
                    Err(_) => {
                        debug!("unable to unpack actions");
                        return;
                    },
                };

                // Filtering out already handled actions
                for (action, _, data) in actions
                    .data()
                    .iter()
                    .filter(|(_, snapshot, _)| *snapshot > previous_last_client_snapshot)
                {
                    let Some(script) = sd.script_action_component.get(action) else {
                        warn!("script for \"{:?}\" not found", action);
                        continue;
                    };

                    let script_data = ScriptSharedData {
                        snapshot: sd.snapshot,
                        actor_pc: SendPtr::new(&sd.actor_pc),
                        actions_packer_pc: SendMutPtr::new(&mut sd.actions_packer_pc),
                        chunk_view_pc: SendPtr::new(&sd.chunk_view_pc),
                        position_ac: SendPtr::new(&sd.position_ac),
                        block_class_label_map: SendPtr::new(&sd.block_class_label_map),
                        class_bc: SendMutPtr::new(&mut sd.class_bc),
                        collision_bcc: SendPtr::new(&sd.collision_bcc),
                    };

                    sd.script_registry.run_script(
                        &script,
                        script_data,
                        ActionInput {
                            action: (*action).into(),
                            actor: Some((*actor).into()),
                            data,
                        },
                    );
                }
            },
        }
    }
}
