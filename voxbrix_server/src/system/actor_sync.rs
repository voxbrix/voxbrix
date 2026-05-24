use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            effect::EffectActorComponent,
            equipment::EquipmentActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
            ActorComponentCleanup,
            ActorComponentPack,
            ComponentPackerSlot,
        },
        actor_class::{
            dimension_acceleration::DimensionAccelerationActorClassComponent,
            drag::DragActorClassComponent,
            health::HealthActorClassComponent,
            model::ModelActorClassComponent,
        },
        player::{
            actor::ActorPlayerComponent,
            client::{
                ClientEvent,
                ClientPlayerComponent,
                SendData,
            },
            dispatches_packer::DispatchesPackerPlayerComponent,
        },
    },
    entity::player::Player,
};
use nohash_hasher::IntSet;
use rayon::prelude::*;
use voxbrix_common::{
    component::dimension_kind::player_chunk_view::PlayerChunkViewDimensionKindComponent,
    entity::{
        actor::Actor,
        chunk::Chunk,
        snapshot::{
            ServerSnapshot,
            MAX_SNAPSHOT_DIFF,
        },
        update::Update,
    },
    messages::{
        client::{
            ClientAcceptMessage,
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

const POSITION_COMPONENT_COUNT: usize = 1;
const SERVER_CONTROLLED_COMPONENT_COUNT: usize = 7;
const CLIENT_CONTROLLED_COMPONENT_COUNT: usize = 2;
const TOTAL_COMPONENT_COUNT: usize = POSITION_COMPONENT_COUNT
    + SERVER_CONTROLLED_COMPONENT_COUNT
    + CLIENT_CONTROLLED_COMPONENT_COUNT;

#[derive(Default)]
struct SharedBuffers {
    packer: Packer,
    updates_packer: UpdatesPacker,
    components: [Vec<u8>; TOTAL_COMPONENT_COUNT],
    component_packers: [ComponentPackerSlot; TOTAL_COMPONENT_COUNT],
    actors_full_update: IntSet<Actor>,
    actors_partial_update: IntSet<Actor>,
}

#[derive(SystemData)]
pub struct ActorSyncSystemData<'a> {
    snapshot: &'a ServerSnapshot,

    dispatches_packer_pc: &'a mut DispatchesPackerPlayerComponent,
    actor_pc: &'a ActorPlayerComponent,
    client_pc: &'a ClientPlayerComponent,
    player_rq: &'a RemovalQueue<Player>,

    class_ac: &'a mut ClassActorComponent,
    effect_ac: &'a mut EffectActorComponent,
    equipment_ac: &'a mut EquipmentActorComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,

    model_acc: &'a mut ModelActorClassComponent,
    health_acc: &'a mut HealthActorClassComponent,
    drag_acc: &'a mut DragActorClassComponent,
    dimension_acceleration_acc: &'a mut DimensionAccelerationActorClassComponent,

    player_chunk_view_dkc: &'a PlayerChunkViewDimensionKindComponent,
}

impl ActorSyncSystemData<'_> {
    pub fn run(self) {
        let Self {
            snapshot,
            dispatches_packer_pc,
            actor_pc,
            client_pc,
            player_rq,
            class_ac,
            effect_ac,
            equipment_ac,
            position_ac,
            velocity_ac,
            orientation_ac,
            model_acc,
            health_acc,
            drag_acc,
            dimension_acceleration_acc,
            player_chunk_view_dkc,
        } = self;

        let snapshot_for_cleanup = *snapshot;

        rayon::join(
            || {
                let cleanups: [&mut dyn ActorComponentCleanup; TOTAL_COMPONENT_COUNT] = [
                    position_ac,
                    effect_ac,
                    equipment_ac,
                    class_ac,
                    velocity_ac,
                    orientation_ac,
                    model_acc,
                    health_acc,
                    drag_acc,
                    dimension_acceleration_acc,
                ];
                cleanups
                    .into_par_iter()
                    .for_each(|c| c.cleanup(snapshot_for_cleanup));
            },
            || {
                dispatches_packer_pc
                    .par_iter_mut()
                    .for_each(|(_player, dispatches_packer)| {
                        dispatches_packer.prepare();
                    });
            },
        );

        // Position is packed separately, it determines the actor sets.
        // Server-controlled components are not filtered by `player_actor`,
        // client-controlled ones are.
        let server_controlled: [&dyn ActorComponentPack; SERVER_CONTROLLED_COMPONENT_COUNT] = [
            class_ac,
            model_acc,
            health_acc,
            effect_ac,
            equipment_ac,
            drag_acc,
            dimension_acceleration_acc,
        ];
        let client_controlled: [&dyn ActorComponentPack; CLIENT_CONTROLLED_COMPONENT_COUNT] =
            [velocity_ac, orientation_ac];

        // Reborrow as shared for concurrent access below.
        let dispatches_packer_pc: &DispatchesPackerPlayerComponent = dispatches_packer_pc;

        client_pc
            .par_iter()
            .filter_map(|(player, client)| {
                let player_actor = actor_pc.get(player)?;
                Some((
                    player,
                    client,
                    player_actor,
                    position_ac.get(player_actor)?.chunk,
                ))
            })
            .for_each_init(
                // Per-worker scratch: created once per Rayon worker per call,
                // reused across every client this worker handles.
                SharedBuffers::default,
                |shared, (player, client, player_actor, position_chunk)| {
                    let dispatches_packer = dispatches_packer_pc
                        .get(player)
                        .expect("dispatches packer is not defined for a player");

                    // Disconnect if last snapshot is too old or client loop is gone.
                    // Players lacking position or Actor will NOT be disconnected.
                    if snapshot.0 - client.last_server_snapshot.0 > MAX_SNAPSHOT_DIFF
                    // TODO after several seconds disconnect Snapshot(0) ones anyway:
                    && client.last_server_snapshot != ServerSnapshot(0)
                        || client.tx.is_disconnected()
                    {
                        player_rq.enqueue(*player);
                        return;
                    }

                    let chunk_view_radius =
                        player_chunk_view_dkc.get(&position_chunk.dimension.kind);

                    let chunk_radius = chunk_view_radius.to_chunk_radius(&position_chunk);

                    let client_is_outdated = client.last_server_snapshot == ServerSnapshot(0)
                        || snapshot.0 - client.last_server_snapshot.0 > MAX_SNAPSHOT_DIFF;

                    let previous_chunk_radius = client
                        .last_confirmed_chunk
                        // Force full update for outdated clients.
                        .filter(|_| !client_is_outdated)
                        // TODO Should be `previous_view` if the view is runtime-variable.
                        .map(|c| chunk_view_radius.to_chunk_radius(&c));

                    // None means full-update mode (no previous view to delta against).
                    let full_data = previous_chunk_radius.is_none();

                    // Chunk in both previous and current view radii; false in full-update mode.
                    let chunk_within_intersection = |chunk: Option<&Chunk>| -> bool {
                        let chunk = match chunk {
                            Some(v) => v,
                            None => return false,
                        };

                        previous_chunk_radius
                            .map(|prev| prev.is_within(chunk) && chunk_radius.is_within(chunk))
                            .unwrap_or(false)
                    };

                    // Newly visible chunks; in full-update mode, all current-view chunks.
                    // TODO optimize?
                    let new_chunks = chunk_radius
                        .into_iter_simple()
                        .filter(|c| previous_chunk_radius.is_none_or(|prev| !prev.is_within(c)));

                    // Persisted intersection; empty in full-update mode (then ignored by pack).
                    // TODO optimize?
                    let intersection_chunks = chunk_radius
                        .into_iter_simple()
                        .filter(|c| previous_chunk_radius.is_some_and(|prev| prev.is_within(c)));

                    // First - for position, rest - for other components.
                    let (position_buf, component_buffers) =
                        shared.components.split_first_mut().unwrap();
                    let (position_packer_slot, component_packer_slots) =
                        shared.component_packers.split_first_mut().unwrap();

                    // Position determines the actor sets shared with all other components.
                    let last_snapshot = client.last_server_snapshot;
                    let position_update = position_ac.update();
                    position_ac.pack(
                        full_data,
                        last_snapshot,
                        player_actor,
                        chunk_within_intersection,
                        new_chunks,
                        intersection_chunks,
                        position_buf,
                        &mut shared.actors_full_update,
                        &mut shared.actors_partial_update,
                        position_packer_slot,
                    );

                    let mut entries: [(Update, &[u8]); TOTAL_COMPONENT_COUNT] =
                        [(Update(u32::MAX), &[]); TOTAL_COMPONENT_COUNT];
                    entries[0] = (position_update, position_buf.as_slice());

                    for (i, ((c, is_client_controlled), (buf, packer_slot))) in server_controlled
                        .iter()
                        .map(|c| (c, false))
                        .chain(client_controlled.iter().map(|c| (c, true)))
                        .zip(
                            component_buffers
                                .iter_mut()
                                .zip(component_packer_slots.iter_mut()),
                        )
                        .enumerate()
                    {
                        let update = c.pack(
                            full_data,
                            last_snapshot,
                            is_client_controlled.then_some(player_actor),
                            &shared.actors_full_update,
                            &shared.actors_partial_update,
                            buf,
                            packer_slot,
                        );
                        entries[1 + i] = (update, buf.as_slice());
                    }

                    let updates = shared.updates_packer.pack_from_slice(&entries);
                    let dispatches = dispatches_packer.packed();

                    let data = ClientAcceptMessage::pack_state(
                        &mut shared.packer,
                        &ServerState {
                            snapshot: *snapshot,
                            last_client_snapshot: client.last_client_snapshot,
                            updates,
                            dispatches,
                        },
                    )
                    .into_bytes();

                    if client
                        .tx
                        .send(ClientEvent::SendDataUnreliable {
                            data: SendData::Owned(data),
                        })
                        .is_err()
                    {
                        player_rq.enqueue(*player);
                    }
                },
            );
    }
}
