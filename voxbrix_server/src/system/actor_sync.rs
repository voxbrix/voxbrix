use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            effect::EffectActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::model::ModelActorClassComponent,
        player::{
            actor::ActorPlayerComponent,
            chunk_view::ChunkViewPlayerComponent,
            client::{
                ClientEvent,
                ClientPlayerComponent,
                SendData,
            },
            dispatches_packer::DispatchesPackerPlayerComponent,
        },
    },
    entity::player::Player,
    BASE_CHANNEL,
};
use voxbrix_common::{
    entity::{
        chunk::Chunk,
        snapshot::{
            ServerSnapshot,
            MAX_SNAPSHOT_DIFF,
        },
    },
    messages::{
        client::{
            ClientAccept,
            ServerState,
        },
        UpdatesPacker,
    },
    pack::Packer,
    resource::removal_queue::RemovalQueue,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ActorSyncSystem;

impl System for ActorSyncSystem {
    type Data<'a> = ActorSyncSystemData<'a>;
}

#[derive(SystemData)]
pub struct ActorSyncSystemData<'a> {
    snapshot: &'a ServerSnapshot,

    dispatches_packer_pc: &'a mut DispatchesPackerPlayerComponent,
    actor_pc: &'a ActorPlayerComponent,
    chunk_view_pc: &'a ChunkViewPlayerComponent,
    client_pc: &'a ClientPlayerComponent,
    player_rq: &'a mut RemovalQueue<Player>,

    class_ac: &'a mut ClassActorComponent,
    effect_ac: &'a mut EffectActorComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,

    model_acc: &'a mut ModelActorClassComponent,

    packer: &'a mut Packer,
    updates_packer: &'a mut UpdatesPacker,
}

impl ActorSyncSystemData<'_> {
    pub fn run(mut self) {
        for (player, player_actor, client) in self
            .actor_pc
            .iter()
            .filter_map(|(player, actor)| Some((player, actor, self.client_pc.get(player)?)))
        {
            // Disconnect player if his last snapshot is too low
            // or if the client loop has been dropped
            if self.snapshot.0 - client.last_server_snapshot.0 > MAX_SNAPSHOT_DIFF
                // TODO after several seconds disconnect Snapshot(0) ones anyway:
                && client.last_server_snapshot != ServerSnapshot(0)
                || client.tx.is_disconnected()
            {
                self.player_rq.enqueue(*player);
                continue;
            }

            let position_chunk = match self.position_ac.get(&player_actor) {
                Some(v) => v.chunk,
                None => continue,
            };

            let chunk_view_radius = match self.chunk_view_pc.get(&player) {
                Some(v) => v.radius,
                None => continue,
            };

            let chunk_radius = position_chunk.radius(chunk_view_radius);

            let client_is_outdated = client.last_server_snapshot == ServerSnapshot(0)
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
                    .into_iter_simple()
                    .filter(|c| !previous_chunk_radius.is_within(c));

                // TODO optimize?
                let intersection_chunks = chunk_radius
                    .into_iter_simple()
                    .filter(|c| previous_chunk_radius.is_within(c));

                self.position_ac.pack_changes(
                    &mut self.updates_packer,
                    *self.snapshot,
                    client.last_server_snapshot,
                    player_actor,
                    chunk_within_intersection,
                    new_chunks,
                    intersection_chunks,
                );

                // Server-controlled components, we pass `None` instead of `player_actor`.
                // These components will not filter out player's own components.
                self.class_ac.pack_changes(
                    &mut self.updates_packer,
                    *self.snapshot,
                    client.last_server_snapshot,
                    None,
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );

                self.model_acc.pack_changes(
                    &mut self.updates_packer,
                    *self.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );

                self.effect_ac.pack_changes(
                    &mut self.updates_packer,
                    *self.snapshot,
                    client.last_server_snapshot,
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );

                // Client-conrolled components, we pass `Some(player_actor)`.
                // These components will filter out player's own components.
                self.velocity_ac.pack_changes(
                    &mut self.updates_packer,
                    *self.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );

                self.orientation_ac.pack_changes(
                    &mut self.updates_packer,
                    *self.snapshot,
                    client.last_server_snapshot,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                    self.position_ac.actors_partial_update(),
                );
            } else {
                // TODO optimize?
                let new_chunks = chunk_radius.into_iter_simple();

                self.position_ac
                    .pack_full(&mut self.updates_packer, player_actor, new_chunks);

                // Server-controlled components, we pass `None` instead of `player_actor`.
                // These components will not filter out player's own components.
                self.class_ac.pack_full(
                    &mut self.updates_packer,
                    None,
                    self.position_ac.actors_full_update(),
                );

                self.model_acc.pack_full(
                    &mut self.updates_packer,
                    None,
                    self.position_ac.actors_full_update(),
                );

                self.effect_ac.pack_full(
                    &mut self.updates_packer,
                    self.position_ac.actors_full_update(),
                );

                // Client-conrolled components, we pass `Some(player_actor)`.
                // These components will filter out player's own components.
                self.velocity_ac.pack_full(
                    &mut self.updates_packer,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                );

                self.orientation_ac.pack_full(
                    &mut self.updates_packer,
                    Some(player_actor),
                    self.position_ac.actors_full_update(),
                );
            }

            let updates = self.updates_packer.pack();
            let dispatches = self
                .dispatches_packer_pc
                .get_mut(player)
                .expect("no dispatches packer found for a player")
                .pack();

            let data = self.packer.pack_to_vec(&ClientAccept::State(ServerState {
                snapshot: *self.snapshot,
                last_client_snapshot: client.last_client_snapshot,
                updates,
                dispatches,
            }));

            if client
                .tx
                .send(ClientEvent::SendDataUnreliable {
                    channel: BASE_CHANNEL,
                    data: SendData::Owned(data),
                })
                .is_err()
            {
                self.player_rq.enqueue(*player);
            }
        }
    }
}
