use crate::{
    component::player::client::{
        ClientEvent,
        SendData,
    },
    server::{
        SendRc,
        SharedEvent,
    },
    system::chunk_activation::ChunkActivationOutcome,
    world::World,
    BASE_CHANNEL,
};
use std::time::Instant;
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

impl World {
    pub fn process(mut self) -> World {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_process_time);
        self.last_process_time = now;

        // Sending chunks to players
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
            for chunk_data in curr_radius.into_iter().filter_map(|chunk| {
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
                        data: SendData::Ref(chunk_data.clone()),
                    })
                    .is_err()
                {
                    self.remove_queue.remove_player(&player);
                }
            }
        }

        self.chunk_activation_system.clear();
        self.chunk_activation_system
            .actor_activations(&self.chunk_activation_ac, &self.position_ac);

        self.position_system.process(
            elapsed,
            &self.class_bc,
            &self.collision_bcc,
            &mut self.position_ac,
            &self.velocity_ac,
            &self.player_ac,
            self.snapshot,
        );

        for (player, player_actor, client) in self
            .actor_pc
            .iter()
            .filter_map(|(player, actor)| Some((player, actor, self.client_pc.get(player)?)))
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

            let position_chunk = match self.position_ac.get(&player_actor) {
                Some(v) => v.chunk,
                None => continue,
            };

            let chunk_view_radius = match self.chunk_view_pc.get(&player) {
                Some(v) => v.radius,
                None => continue,
            };

            let chunk_radius = position_chunk.radius(chunk_view_radius);

            let client_is_outdated = client.last_server_snapshot == Snapshot(0)
                || self.snapshot.0 - client.last_server_snapshot.0 > MAX_SNAPSHOT_DIFF;

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
                    .into_iter()
                    .filter(|c| !previous_chunk_radius.is_within(c));

                // TODO optimize?
                let intersection_chunks = chunk_radius
                    .into_iter()
                    .filter(|c| previous_chunk_radius.is_within(c));

                self.position_ac.pack_changes(
                    &mut self.server_state,
                    self.snapshot,
                    client.last_server_snapshot,
                    player_actor,
                    chunk_within_intersection,
                    new_chunks,
                    intersection_chunks,
                );

                // Server-controlled components, we pass `None` instead of `player_actor`.
                // These components will not filter out player's own components.
                self.class_ac.pack_changes(
                    &mut self.server_state,
                    self.snapshot,
                    client.last_server_snapshot,
                    None,
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );

                self.model_acc.pack_changes(
                    &mut self.server_state,
                    self.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );

                // Client-conrolled components, we pass `Some(player_actor)`.
                // These components will filter out player's own components.
                self.velocity_ac.pack_changes(
                    &mut self.server_state,
                    self.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );

                self.orientation_ac.pack_changes(
                    &mut self.server_state,
                    self.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );
            } else {
                // TODO optimize?
                let new_chunks = chunk_radius.into_iter();

                self.position_ac
                    .pack_full(&mut self.server_state, player_actor, new_chunks);

                // Server-controlled components, we pass `None` instead of `player_actor`.
                // These components will not filter out player's own components.
                self.class_ac.pack_full(
                    &mut self.server_state,
                    None,
                    self.position_ac.actors_full_update(),
                );

                self.model_acc.pack_full(
                    &mut self.server_state,
                    None,
                    self.position_ac.actors_full_update(),
                );

                // Client-conrolled components, we pass `Some(player_actor)`.
                // These components will filter out player's own components.
                self.velocity_ac.pack_full(
                    &mut self.server_state,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                );

                self.orientation_ac.pack_full(
                    &mut self.server_state,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                );
            }

            let data = ClientAccept::pack_state(
                self.snapshot,
                client.last_client_snapshot,
                &mut self.server_state,
                &mut self.packer,
            );

            if client
                .tx
                .send(ClientEvent::SendDataUnreliable {
                    channel: BASE_CHANNEL,
                    data: SendData::Owned(data),
                })
                .is_err()
            {
                self.remove_queue.remove_player(player);
            }
        }

        self.chunk_activation_system.apply(
            self.shared,
            &mut self.status_cc,
            &mut self.class_bc,
            &mut self.cache_cc,
            move |chunk, activation_outcome, packer| {
                match activation_outcome {
                    ChunkActivationOutcome::ChunkActivated(block_classes) => {
                        let data = ChunkData {
                            chunk,
                            block_classes,
                        };

                        let data_encoded =
                            SendRc::new(packer.pack_to_vec(&ClientAccept::ChunkData(data.clone())));

                        let _ = self
                            .shared
                            .event_tx
                            .send(SharedEvent::ChunkLoaded { data, data_encoded });
                    },
                    ChunkActivationOutcome::ChunkNeedsGeneration => {
                        let _ = self
                            .shared
                            .event_tx
                            .send(SharedEvent::ChunkGeneration(chunk));
                    },
                }
            },
        );

        self.snapshot = self.snapshot.next();

        self
    }
}
