use crate::{
    component::{
        action::handler::{
            Alteration,
            Condition,
            Source,
            Target,
        },
        player::chunk_update::{
            ChunkUpdate,
            FullChunkView,
        },
    },
    entity::player::Player,
    server_loop::data::{
        ScriptSharedDataRef,
        SharedData,
    },
};
use log::debug;
use server_loop_api::ActionInput;
use std::mem;
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::Snapshot,
    },
    messages::{
        server::ServerAccept,
        ActionsPacked,
        ActionsUnpacker,
        StatePacked,
        StateUnpacker,
    },
};
use voxbrix_protocol::server::Packet;

/// If the state is valid, returns previous last client snapshot for filtering actions.
fn handle_state(
    state_unpacker: &mut StateUnpacker,
    sd: &mut SharedData,
    player: Player,
    state: StatePacked,
    last_client_snapshot: Snapshot,
    last_server_snapshot: Snapshot,
) -> Option<Snapshot> {
    let state = match state_unpacker.unpack_state(state) {
        Ok(v) => v,
        Err(_) => {
            debug!("skipping corrupted state");
            return None;
        },
    };

    // TODO: there could be sequential messaged on the wire that have the same state
    // changes duplicated. Make sure that those do not duplicate in the components.
    let actor = match sd.actor_pc.get(&player) {
        Some(a) => a,
        None => return None,
    };

    let client = match sd.client_pc.get_mut(&player) {
        Some(c) => c,
        None => return None,
    };

    if client.last_client_snapshot >= last_client_snapshot {
        return None;
    }

    let previous_last_client_snapshot = client.last_client_snapshot;

    client.last_server_snapshot = last_server_snapshot;
    client.last_client_snapshot = last_client_snapshot;

    sd.velocity_ac.unpack_player(actor, &state, sd.snapshot);
    sd.orientation_ac.unpack_player(actor, &state, sd.snapshot);

    sd.position_ac
        .unpack_player_with(actor, &state, sd.snapshot, |old_value, new_value| {
            let chunk = match new_value {
                Some(v) => v.chunk,
                None => return,
            };

            sd.client_pc.get_mut(&player).unwrap().last_confirmed_chunk = Some(chunk);

            if old_value.is_none() || old_value.is_some() && old_value.unwrap().chunk != chunk {
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
        });

    // Pruning confirmed Server -> Client actions.
    sd.actions_packer_pc
        .get_mut(&player)
        .expect("actions packer not found for a player")
        .confirm_snapshot(last_server_snapshot);

    Some(previous_last_client_snapshot)
}

/// If the state is valid, returns previous last client snapshot for filtering actions.
fn handle_actions(
    actions_unpacker: &mut ActionsUnpacker,
    sd: &mut SharedData,
    player: Player,
    actions: ActionsPacked<'_>,
    previous_last_client_snapshot: Snapshot,
) {
    let actor = *sd
        .actor_pc
        .get(&player)
        .expect("player missing actor must be caught earlier");

    let actions = match actions_unpacker.unpack_actions(actions) {
        Ok(v) => v,
        Err(_) => {
            debug!("unable to unpack actions");
            return;
        },
    };

    fn condition_valid(sd: &SharedData, condition: &Condition, source: &Actor) -> bool {
        match condition {
            Condition::Always => true,
            Condition::SourceActorHasNoEffect(effect) => !sd.effect_ac.has_effect(*source, *effect),
            Condition::And(conditions) => conditions.iter().all(|c| condition_valid(sd, c, source)),
            Condition::Or(conditions) => conditions.iter().any(|c| condition_valid(sd, c, source)),
        }
    }

    // Filtering out already handled actions
    for (action, _, data) in actions
        .data()
        .iter()
        .filter(|(_, snapshot, _)| *snapshot > previous_last_client_snapshot)
    {
        let handler_set = sd.handler_action_component.get(action);

        for handler in handler_set.iter() {
            if !condition_valid(sd, &handler.condition, &actor) {
                continue;
            }

            for alteration in handler.alterations.iter() {
                match alteration {
                    Alteration::ApplyEffect {
                        source,
                        target,
                        effect,
                    } => {
                        let source = match source {
                            Source::Actor => Some(actor),
                            Source::World => None,
                        };

                        let target = match target {
                            Target::Actor => actor,
                        };

                        sd.effect_ac.add(target, *effect, source);
                    },
                    Alteration::RemoveSourceActorEffect { effect } => {
                        sd.effect_ac.remove_any_source(actor, *effect);
                    },
                    Alteration::CreateProjectile { actor_class } => {
                        panic!("unimplemented projectile creation for {:?}", actor_class);
                    },
                    Alteration::Scripted { script } => {
                        let script_data = ScriptSharedDataRef {
                            snapshot: sd.snapshot,
                            actor_pc: &sd.actor_pc,
                            actions_packer_pc: &mut sd.actions_packer_pc,
                            chunk_view_pc: &sd.chunk_view_pc,
                            position_ac: &sd.position_ac,
                            label_library: &sd.label_library,
                            class_bc: &mut sd.class_bc,
                            collision_bcc: &sd.collision_bcc,
                        }
                        .into_static();

                        sd.script_registry.run_script(
                            &script,
                            script_data,
                            ActionInput {
                                action: (*action).into(),
                                actor: Some(actor.into()),
                                data,
                            },
                        );
                    },
                }
            }
        }
    }
}

pub struct PlayerEvent<'a> {
    pub shared_data: &'a mut SharedData,
    pub player: Player,
    pub data: Packet,
}

impl PlayerEvent<'_> {
    pub fn run(self) {
        let Self {
            shared_data: mut sd,
            player,
            data,
        } = self;

        let mut packer = mem::take(&mut sd.packer);

        match packer.unpack::<ServerAccept>(data.as_ref()) {
            Ok(event) => {
                match event {
                    ServerAccept::State {
                        snapshot: last_client_snapshot,
                        last_server_snapshot,
                        state,
                        actions,
                    } => {
                        let mut state_unpacker = mem::take(&mut sd.state_unpacker);
                        let prev_lcs = handle_state(
                            &mut state_unpacker,
                            &mut sd,
                            player,
                            state,
                            last_client_snapshot,
                            last_server_snapshot,
                        );
                        sd.state_unpacker = state_unpacker;

                        if let Some(previous_last_client_snapshot) = prev_lcs {
                            let mut actions_unpacker = mem::take(&mut sd.actions_unpacker);
                            handle_actions(
                                &mut actions_unpacker,
                                &mut sd,
                                player,
                                actions,
                                previous_last_client_snapshot,
                            );
                            sd.actions_unpacker = actions_unpacker;
                        }
                    },
                }
            },
            Err(_) => {
                debug!(
                    "server_loop: unable to parse data from player {:?} on base channel",
                    player
                );
                return;
            },
        };

        sd.packer = packer;
    }
}
