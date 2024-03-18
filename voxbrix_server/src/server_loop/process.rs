use crate::{
    component::player::client::{
        ClientEvent,
        SendData,
    },
    server_loop::{
        data::SharedData,
        SharedEvent,
    },
    system::chunk_activation::ChunkActivationOutcome,
    BASE_CHANNEL,
};
use std::{
    sync::Arc,
    time::Instant,
};
use tokio::runtime::Handle;
use voxbrix_common::{
    entity::{
        chunk::Chunk,
        snapshot::{
            Snapshot,
            MAX_SNAPSHOT_DIFF,
        },
    },
    messages::client::ClientAccept,
    ChunkData,
};

pub struct Process<'a> {
    pub shared_data: &'a mut SharedData,
    pub rt_handle: Handle,
}

impl Process<'_> {
    pub fn run(self) {
        let Self {
            shared_data: sd,
            rt_handle,
        } = self;

        let now = Instant::now();
        let elapsed = now.saturating_duration_since(sd.last_process_time);
        sd.last_process_time = now;

        // Sending chunks to players
        for (player, client, prev_radius, curr_radius) in
            sd.chunk_update_pc
                .drain()
                .filter_map(|(player, prev_view)| {
                    let actor = sd.actor_pc.get(&player)?;
                    let client = sd.client_pc.get(&player)?;
                    let position = sd.position_ac.get(&actor)?;
                    let curr_view = sd.chunk_view_pc.get(&player)?;
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

                sd.cache_cc.get(&chunk)
            }) {
                if client
                    .tx
                    .send(ClientEvent::SendDataReliable {
                        channel: BASE_CHANNEL,
                        data: SendData::Arc(chunk_data.clone().into_inner()),
                    })
                    .is_err()
                {
                    sd.remove_queue.remove_player(&player);
                }
            }
        }

        sd.chunk_activation_system.clear();
        sd.chunk_activation_system
            .actor_activations(&sd.chunk_activation_ac, &sd.position_ac);

        sd.position_system.process(
            elapsed,
            &sd.class_bc,
            &sd.collision_bcc,
            &mut sd.position_ac,
            &sd.velocity_ac,
            &sd.player_ac,
            sd.snapshot,
        );

        for (player, player_actor, client) in sd.actor_pc
            .iter()
            .filter_map(|(player, actor)| Some((player, actor, sd.client_pc.get(player)?)))
        {
            // Disconnect player if his last snapshot is too low
            /*if snapshot.0 - client.last_server_snapshot.0 > MAX_SNAPSHOT_DIFF
                // TODO after several seconds disconnect Snapshot(0) ones anyway:
                && client.last_server_snapshot != Snapshot(0) {
                let _ = local
                    .event_tx
                    .send(ServerEvent::RemovePlayer { player: *player });

                continue;
            }*/

            let position_chunk = match sd.position_ac.get(&player_actor) {
                Some(v) => v.chunk,
                None => continue,
            };

            let chunk_view_radius = match sd.chunk_view_pc.get(&player) {
                Some(v) => v.radius,
                None => continue,
            };

            let chunk_radius = position_chunk.radius(chunk_view_radius);

            let client_is_outdated = client.last_server_snapshot == Snapshot(0)
                || sd.snapshot.0 - client.last_server_snapshot.0 > MAX_SNAPSHOT_DIFF;

            if let Some(previous_chunk_radius) = client
                .last_confirmed_chunk
                // Enforces full update for the outdated clients
                .filter(|_| !client_is_outdated)
                // TODO Should be `previous_view` if the view is runtime-variable.
                .map(|c| c.radius(chunk_view_radius))
            {
                let chunk_within_intersection = |chunk: Option<&Chunk>| -> bool {
                    let chunk = match chunk {
                        Some(v) => v,
                        None => return false,
                    };

                    previous_chunk_radius.is_within(chunk) && chunk_radius.is_within(chunk)
                };

                // TODO optimize?
                let new_chunks = chunk_radius
                    .into_iter_simple()
                    .filter(|c| !previous_chunk_radius.is_within(c));

                // TODO optimize?
                let intersection_chunks = chunk_radius
                    .into_iter_simple()
                    .filter(|c| previous_chunk_radius.is_within(c));

                sd.position_ac.pack_changes(
                    &mut sd.state_packer,
                    sd.snapshot,
                    client.last_server_snapshot,
                    player_actor,
                    chunk_within_intersection,
                    new_chunks,
                    intersection_chunks,
                );

                // Server-controlled components, we pass `None` instead of `player_actor`.
                // These components will not filter out player's own components.
                sd.class_ac.pack_changes(
                    &mut sd.state_packer,
                    sd.snapshot,
                    client.last_server_snapshot,
                    None,
                    sd.position_ac.actors_full_update(),
                    sd.position_ac.actors_partial_update(),
                );

                sd.model_acc.pack_changes(
                    &mut sd.state_packer,
                    sd.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    sd.position_ac.actors_full_update(),
                    sd.position_ac.actors_partial_update(),
                );

                // Client-conrolled components, we pass `Some(player_actor)`.
                // These components will filter out player's own components.
                sd.velocity_ac.pack_changes(
                    &mut sd.state_packer,
                    sd.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    sd.position_ac.actors_full_update(),
                    sd.position_ac.actors_partial_update(),
                );

                sd.orientation_ac.pack_changes(
                    &mut sd.state_packer,
                    sd.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    sd.position_ac.actors_full_update(),
                    sd.position_ac.actors_partial_update(),
                );
            } else {
                // TODO optimize?
                let new_chunks = chunk_radius.into_iter_simple();

                sd.position_ac
                    .pack_full(&mut sd.state_packer, player_actor, new_chunks);

                // Server-controlled components, we pass `None` instead of `player_actor`.
                // These components will not filter out player's own components.
                sd.class_ac.pack_full(
                    &mut sd.state_packer,
                    None,
                    sd.position_ac.actors_full_update(),
                );

                sd.model_acc.pack_full(
                    &mut sd.state_packer,
                    None,
                    sd.position_ac.actors_full_update(),
                );

                // Client-conrolled components, we pass `Some(player_actor)`.
                // These components will filter out player's own components.
                sd.velocity_ac.pack_full(
                    &mut sd.state_packer,
                    Some(player_actor),
                    sd.position_ac.actors_full_update(),
                );

                sd.orientation_ac.pack_full(
                    &mut sd.state_packer,
                    Some(player_actor),
                    sd.position_ac.actors_full_update(),
                );
            }

            let state = sd.state_packer.pack_state();

            let data = sd.packer.pack_to_vec(&ClientAccept::State {
                snapshot: sd.snapshot,
                last_client_snapshot: client.last_client_snapshot,
                state,
            });

            if client
                .tx
                .send(ClientEvent::SendDataUnreliable {
                    channel: BASE_CHANNEL,
                    data: SendData::Owned(data),
                })
                .is_err()
            {
                sd.remove_queue.remove_player(player);
            }
        }

        let shared_event_tx = sd.shared_event_tx.clone();

        sd.chunk_activation_system.apply(
            &mut sd.database,
            &mut sd.status_cc,
            &mut sd.class_bc,
            &mut sd.cache_cc,
            move |chunk, activation_outcome, packer| {
                match activation_outcome {
                    ChunkActivationOutcome::ChunkActivated(block_classes) => {
                        let data = ChunkData {
                            chunk,
                            block_classes,
                        };

                        let data_encoded =
                            Arc::new(packer.pack_to_vec(&ClientAccept::ChunkData(data.clone())));

                        let _ =
                            shared_event_tx.send(SharedEvent::ChunkLoaded { data, data_encoded });
                    },
                    ChunkActivationOutcome::ChunkNeedsGeneration => {
                        let _ = shared_event_tx.send(SharedEvent::ChunkGeneration(chunk));
                    },
                }
            },
            &rt_handle,
        );

        sd.snapshot = sd.snapshot.next();
    }
}
