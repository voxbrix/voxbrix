use crate::{
    client::{
        ClientEvent,
        SendData,
    },
    component::{
        actor::{
            chunk_ticket::{
                ActorChunkTicket,
                ChunkTicketActorComponent,
            },
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            player::PlayerActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::model::ModelActorClassComponent,
        chunk::{
            cache::CacheChunkComponent,
            status::{
                ChunkStatus,
                StatusChunkComponent,
            },
        },
        player::{
            actor::ActorPlayerComponent,
            client::ClientPlayerComponent,
        },
    },
    entity::{
        actor::ActorRegistry,
        player::Player,
    },
    storage::{
        IntoData,
        IntoDataSized,
        StorageThread,
    },
    system::{
        chunk_ticket::ChunkTicketSystem,
        position::PositionSystem,
    },
    Local,
    Shared,
    BASE_CHANNEL,
    BLOCK_CLASS_TABLE,
    PLAYER_CHUNK_TICKET_RADIUS,
    PROCESS_INTERVAL,
};
use flume::Receiver as SharedReceiver;
use futures_lite::stream::{
    self,
    StreamExt,
};
use local_channel::mpsc::{
    Receiver,
    Sender,
};
use log::debug;
use redb::ReadableTable;
use std::{
    rc::Rc,
    time::Instant,
};
use tokio::time::{
    self,
    MissedTickBehavior,
};
use voxbrix_common::{
    assets::{
        ACTOR_MODEL_LIST_PATH,
        STATE_COMPONENTS_PATH,
    },
    component::{
        block::{
            class::ClassBlockComponent,
            BlocksVec,
        },
        block_class::collision::{
            Collision,
            CollisionBlockClassComponent,
        },
    },
    entity::{
        actor_model::ActorModel,
        block::{
            BLOCKS_IN_CHUNK_USIZE,
            BLOCKS_IN_CHUNK_LAYER_USIZE,
        },
        chunk::Chunk,
        snapshot::{
            Snapshot,
            MAX_SNAPSHOT_DIFF,
        },
        state_component::StateComponent,
    },
    messages::{
        client::ClientAccept,
        server::ServerAccept,
        StatePacker,
    },
    pack::Packer,
    system::{
        actor_class_loading::ActorClassLoadingSystem,
        block_class_loading::BlockClassLoadingSystem,
        list_loading::List,
    },
    ChunkData,
};
use voxbrix_protocol::{
    server::Packet,
    Channel,
};

pub enum SharedEvent {
    ChunkLoaded {
        data: ChunkData,
        data_encoded: SendRc<Vec<u8>>,
    },
}

// Server loop input
pub enum ServerEvent {
    Process,
    AddPlayer {
        player: Player,
        client_tx: Sender<ClientEvent>,
    },
    PlayerEvent {
        player: Player,
        channel: Channel,
        data: Packet,
    },
    RemovePlayer {
        player: Player,
    },
    SharedEvent(SharedEvent),
    ServerConnectionClosed,
}

pub struct Client {
    tx: Sender<ClientEvent>,
    // The last server snapshot received by the client
    last_server_snapshot: Snapshot,
    // The last client snapshot received from the client
    last_client_snapshot: Snapshot,
    last_confirmed_chunk: Option<Chunk>,
}

/// Packs data into Rc in one thread and extract it in another
pub struct SendRc<T>(Rc<T>);

impl<T> SendRc<T>
where
    T: Send,
{
    pub fn new(data: T) -> Self {
        Self(Rc::new(data))
    }

    pub fn extract(self) -> Rc<T> {
        self.0
    }
}

// Safe, as the Rc counter in the container can not be incremented (clone)
// and can be decremented (drop) only once, with dropping the container
unsafe impl<T: Send> Send for SendRc<T> {}

// Safe, references to the container can safely be passed between threads
// because one can only get access to the underlying Rc by consuming
// the container, which does not have Clone
unsafe impl<T: Sync> Sync for SendRc<T> {}

pub async fn run(
    local: &'static Local,
    shared: &'static Shared,
    event_rx: Receiver<ServerEvent>,
    event_shared_rx: SharedReceiver<SharedEvent>,
) {
    let actor_class_loading_system = ActorClassLoadingSystem::load_data()
        .await
        .expect("loading actor classes");

    let block_class_loading_system = BlockClassLoadingSystem::load_data()
        .await
        .expect("loading block classes");

    let mut packer = Packer::new();

    let mut actor_registry = ActorRegistry::new();

    let mut client_pc = ClientPlayerComponent::new();
    let mut actor_pc = ActorPlayerComponent::new();

    let state_components_label_map = List::load(STATE_COMPONENTS_PATH)
        .await
        .expect("state component list not found")
        .into_label_map(StateComponent::from_usize);

    let mut class_ac =
        ClassActorComponent::new(state_components_label_map.get("actor_class").unwrap());
    let mut position_ac =
        PositionActorComponent::new(state_components_label_map.get("actor_position").unwrap());
    let mut velocity_ac =
        VelocityActorComponent::new(state_components_label_map.get("actor_velocity").unwrap());
    let mut orientation_ac = OrientationActorComponent::new(
        state_components_label_map.get("actor_orientation").unwrap(),
    );
    let mut player_ac = PlayerActorComponent::new();
    let mut chunk_ticket_ac = ChunkTicketActorComponent::new();

    let mut model_acc =
        ModelActorClassComponent::new(state_components_label_map.get("actor_model").unwrap());

    let mut status_cc = StatusChunkComponent::new();
    let mut cache_cc = CacheChunkComponent::new();
    let mut chunk_ticket_system = ChunkTicketSystem::new();

    let mut class_bc = ClassBlockComponent::new();
    let mut collision_bcc = CollisionBlockClassComponent::new();

    let mut position_system = PositionSystem::new();

    let actor_model_label_map = List::load(ACTOR_MODEL_LIST_PATH)
        .await
        .expect("loading actor model label map")
        .into_label_map(ActorModel::from_usize);

    actor_class_loading_system
        .load_component("model", &mut model_acc, |desc: String| {
            actor_model_label_map.get(&desc).ok_or_else(|| {
                anyhow::Error::msg(format!("model \"{}\" not found in the model list", desc))
            })
        })
        .expect("unable to load collision block class component");

    let actor_class_label_map = actor_class_loading_system.into_label_map();

    block_class_loading_system
        .load_component("collision", &mut collision_bcc, |desc: Collision| Ok(desc))
        .expect("unable to load collision block class component");

    let block_class_label_map = block_class_loading_system.into_label_map();

    let mut send_status_interval = time::interval(PROCESS_INTERVAL);
    send_status_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut stream = stream::poll_fn(|cx| {
        send_status_interval
            .poll_tick(cx)
            .map(|_| Some(ServerEvent::Process))
    })
    .or(event_rx)
    .or(event_shared_rx.stream().map(ServerEvent::SharedEvent));

    let storage = StorageThread::new();

    let mut snapshot = Snapshot(1);

    let mut server_state = StatePacker::new();

    let mut last_process_time = Instant::now();

    while let Some(event) = stream.next().await {
        // TODO entity deletion here
        match event {
            ServerEvent::Process => {
                chunk_ticket_system.clear();
                chunk_ticket_system.actor_tickets(&chunk_ticket_ac);

                let now = Instant::now();
                let elapsed = now.saturating_duration_since(last_process_time);
                last_process_time = now;

                position_system.process(
                    elapsed,
                    &class_bc,
                    &collision_bcc,
                    &mut position_ac,
                    &velocity_ac,
                    &player_ac,
                    snapshot,
                );

                for (player, player_actor, client) in actor_pc
                    .iter()
                    .filter_map(|(player, actor)| Some((player, actor, client_pc.get(player)?)))
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

                    let position_chunk = match position_ac.get(&player_actor) {
                        Some(v) => v.chunk,
                        None => continue,
                    };

                    let chunk_radius = position_chunk.radius(PLAYER_CHUNK_TICKET_RADIUS);

                    let client_is_outdated = client.last_server_snapshot == Snapshot(0)
                        || snapshot.0 - client.last_server_snapshot.0 > MAX_SNAPSHOT_DIFF;

                    if let Some(previous_chunk_radius) = client
                        .last_confirmed_chunk
                        // Enforces full update for the outdated clients
                        .filter(|_| !client_is_outdated)
                        .map(|c| c.radius(PLAYER_CHUNK_TICKET_RADIUS))
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

                        position_ac.pack_changes(
                            &mut server_state,
                            snapshot,
                            client.last_server_snapshot,
                            player_actor,
                            chunk_within_intersection,
                            new_chunks,
                            intersection_chunks,
                        );

                        // Server-controlled components, we pass `None` instead of `player_actor`.
                        // These components will not filter out player's own components.
                        class_ac.pack_changes(
                            &mut server_state,
                            snapshot,
                            client.last_server_snapshot,
                            None,
                            position_ac.actors_full_update(),
                            position_ac.actors_partial_update(),
                        );

                        model_acc.pack_changes(
                            &mut server_state,
                            snapshot,
                            client.last_server_snapshot,
                            Some(player_actor),
                            position_ac.actors_full_update(),
                            position_ac.actors_partial_update(),
                        );

                        // Client-conrolled components, we pass `Some(player_actor)`.
                        // These components will filter out player's own components.
                        velocity_ac.pack_changes(
                            &mut server_state,
                            snapshot,
                            client.last_server_snapshot,
                            Some(player_actor),
                            position_ac.actors_full_update(),
                            position_ac.actors_partial_update(),
                        );

                        orientation_ac.pack_changes(
                            &mut server_state,
                            snapshot,
                            client.last_server_snapshot,
                            Some(player_actor),
                            position_ac.actors_full_update(),
                            position_ac.actors_partial_update(),
                        );
                    } else {
                        // TODO optimize?
                        let new_chunks = chunk_radius
                            .into_iter();

                        position_ac.pack_full(
                            &mut server_state,
                            player_actor,
                            new_chunks,
                        );

                        // Server-controlled components, we pass `None` instead of `player_actor`.
                        // These components will not filter out player's own components.
                        class_ac.pack_full(
                            &mut server_state,
                            None,
                            position_ac.actors_full_update(),
                        );

                        model_acc.pack_full(
                            &mut server_state,
                            None,
                            position_ac.actors_full_update(),
                        );

                        // Client-conrolled components, we pass `Some(player_actor)`.
                        // These components will filter out player's own components.
                        velocity_ac.pack_full(
                            &mut server_state,
                            Some(player_actor),
                            position_ac.actors_full_update(),
                        );

                        orientation_ac.pack_full(
                            &mut server_state,
                            Some(player_actor),
                            position_ac.actors_full_update(),
                        );
                    }

                    let data = ClientAccept::pack_state(
                        snapshot,
                        client.last_client_snapshot,
                        &mut server_state,
                        &mut packer,
                    );

                    if client
                        .tx
                        .send(ClientEvent::SendDataUnreliable {
                            channel: BASE_CHANNEL,
                            data: SendData::Owned(data),
                        })
                        .is_err()
                    {
                        let _ = local
                            .event_tx
                            .send(ServerEvent::RemovePlayer { player: *player });
                    }
                }

                let air = block_class_label_map.get("air").unwrap();
                let grass = block_class_label_map.get("grass").unwrap();
                let stone = block_class_label_map.get("stone").unwrap();
                chunk_ticket_system.apply(
                    &mut status_cc,
                    &mut class_bc,
                    &mut cache_cc,
                    move |chunk| {
                        let mut packer = Packer::new();

                        let block_classes = {
                            let db_read = shared.database.begin_read().unwrap();
                            let table = db_read
                                .open_table(BLOCK_CLASS_TABLE)
                                .expect("server_loop: database read");
                            table
                                .get(chunk.into_data_sized())
                                .unwrap()
                                .map(|bytes| bytes.value().into_inner(&mut packer))
                        }
                        .unwrap_or_else(|| {
                            let block_classes = if chunk.position[2] == -1 {
                                let mut chunk_blocks = vec![stone; BLOCKS_IN_CHUNK_USIZE];
                                for block_class in (&mut chunk_blocks
                                    [BLOCKS_IN_CHUNK_USIZE - BLOCKS_IN_CHUNK_LAYER_USIZE .. BLOCKS_IN_CHUNK_USIZE])
                                    .iter_mut()
                                {
                                    *block_class = grass;
                                }
                                BlocksVec::new(chunk_blocks)
                            } else if chunk.position[2] < -1 {
                                BlocksVec::new(vec![stone; BLOCKS_IN_CHUNK_USIZE])
                            } else {
                                BlocksVec::new(vec![air; BLOCKS_IN_CHUNK_USIZE])
                            };

                            let db_write = shared.database.begin_write().unwrap();
                            {
                                let mut table = db_write.open_table(BLOCK_CLASS_TABLE).unwrap();
                                table
                                    .insert(
                                        chunk.into_data_sized(),
                                        block_classes.into_data(&mut packer),
                                    )
                                    .expect("server_loop: database write");
                            }
                            db_write.commit().unwrap();

                            block_classes
                        });

                        let data = ChunkData {
                            chunk: *chunk,
                            block_classes,
                        };

                        let data_encoded =
                            SendRc::new(packer.pack_to_vec(&ClientAccept::ChunkData(data.clone())));

                        let _ = shared
                            .event_tx
                            .send(SharedEvent::ChunkLoaded { data, data_encoded });
                    },
                );

                snapshot = snapshot.next();
            },
            ServerEvent::AddPlayer {
                player,
                client_tx: tx,
            } => {
                let tx_init = tx.clone();
                let actor = actor_registry.add();

                class_ac.insert(actor, actor_class_label_map.get("human").unwrap(), snapshot);

                player_ac.insert(actor, player);

                client_pc.insert(
                    player,
                    Client {
                        tx,
                        last_server_snapshot: Snapshot(0),
                        last_client_snapshot: Snapshot(0),
                        last_confirmed_chunk: None,
                    },
                );
                actor_pc.insert(player, actor);

                if tx_init.send(ClientEvent::AssignActor { actor }).is_err() {
                    // TODO consider removing player instantly?
                    let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
                }
            },
            ServerEvent::PlayerEvent {
                player,
                channel,
                data,
            } => {
                if channel == BASE_CHANNEL {
                    let event = match packer.unpack::<ServerAccept>(data.as_ref()) {
                        Ok(e) => e,
                        Err(_) => {
                            debug!(
                                "server_loop: unable to parse data from player {:?} on channel {}",
                                player, channel
                            );
                            continue;
                        },
                    };

                    match event {
                        ServerAccept::State {
                            snapshot: last_client_snapshot,
                            last_server_snapshot,
                            state,
                        } => {
                            let actor = match actor_pc.get(&player) {
                                Some(a) => a,
                                None => continue,
                            };

                            let client = match client_pc.get_mut(&player) {
                                Some(c) => c,
                                None => continue,
                            };

                            if client.last_client_snapshot >= last_client_snapshot {
                                continue;
                            }

                            client.last_server_snapshot = last_server_snapshot;
                            client.last_client_snapshot = last_client_snapshot;

                            velocity_ac.unpack_player(actor, &state, snapshot);
                            orientation_ac.unpack_player(actor, &state, snapshot);

                            position_ac.unpack_player_with(
                                actor,
                                &state,
                                snapshot,
                                |old_value, new_value| {
                                    let chunk = match new_value {
                                        Some(v) => v.chunk,
                                        None => return,
                                    };

                                    client_pc.get_mut(&player).unwrap().last_confirmed_chunk =
                                        Some(chunk);

                                    if old_value.is_none()
                                        || old_value.is_some() && old_value.unwrap().chunk != chunk
                                    {
                                        let curr_radius = chunk.radius(PLAYER_CHUNK_TICKET_RADIUS);

                                        let prev_radius = chunk_ticket_ac
                                            .insert(
                                                *actor,
                                                ActorChunkTicket {
                                                    chunk,
                                                    radius: PLAYER_CHUNK_TICKET_RADIUS,
                                                },
                                            )
                                            .map(|c| c.chunk.radius(c.radius));

                                        for chunk_data in
                                            curr_radius.into_iter().filter_map(|chunk| {
                                                if let Some(prev_radius) = &prev_radius {
                                                    if prev_radius.is_within(&chunk) {
                                                        return None;
                                                    }
                                                }

                                                cache_cc.get(&chunk)
                                            })
                                        {
                                            if let Some(client) = client_pc.get(&player) {
                                                if client
                                                    .tx
                                                    .send(ClientEvent::SendDataReliable {
                                                        channel: BASE_CHANNEL,
                                                        data: SendData::Ref(chunk_data.clone()),
                                                    })
                                                    .is_err()
                                                {
                                                    let _ = local
                                                        .event_tx
                                                        .send(ServerEvent::RemovePlayer { player });
                                                }
                                            }
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
                            if let Some(block_class_ref) = class_bc
                                .get_mut_chunk(&chunk)
                                .map(|blocks| blocks.get_mut(block))
                            {
                                *block_class_ref = block_class;

                                let data_buf =
                                    Rc::new(packer.pack_to_vec(&ClientAccept::AlterBlock {
                                        chunk,
                                        block,
                                        block_class,
                                    }));

                                for (player, client) in
                                    actor_pc.iter().filter_map(|(player, actor)| {
                                        let ticket = chunk_ticket_ac.get(actor)?;
                                        ticket
                                            .chunk
                                            .radius(ticket.radius)
                                            .is_within(&chunk)
                                            .then_some(())?;
                                        let client = client_pc.get(player)?;
                                        Some((player, client))
                                    })
                                {
                                    if client
                                        .tx
                                        .send(ClientEvent::SendDataReliable {
                                            channel: BASE_CHANNEL,
                                            data: SendData::Ref(data_buf.clone()),
                                        })
                                        .is_err()
                                    {
                                        let _ = local
                                            .event_tx
                                            .send(ServerEvent::RemovePlayer { player: *player });
                                    }
                                }

                                // TODO unify block alterations in Process tick
                                // and update cache there
                                // possibly also unblock/rayon, this takes around 1ms for existence_ach
                                // chunk
                                let blocks_cache = class_bc.get_chunk(&chunk).unwrap().clone();

                                let cache_data = ClientAccept::ChunkData(ChunkData {
                                    chunk,
                                    block_classes: blocks_cache,
                                });

                                cache_cc.insert(chunk, Rc::new(packer.pack_to_vec(&cache_data)));

                                let blocks_cache = match cache_data {
                                    ClientAccept::ChunkData(b) => b.block_classes,
                                    _ => panic!(),
                                };

                                storage.execute(move || {
                                    let mut packer = Packer::new();
                                    let db_write = shared.database.begin_write().unwrap();
                                    {
                                        let mut table =
                                            db_write.open_table(BLOCK_CLASS_TABLE).unwrap();

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
            },
            ServerEvent::RemovePlayer { player } => {
                client_pc.remove(&player);
                if let Some(actor) = actor_pc.remove(&player) {
                    actor_registry.remove(&actor);
                    class_ac.remove(&actor, snapshot);
                    position_ac.remove(&actor, snapshot);
                    velocity_ac.remove(&actor, snapshot);
                    orientation_ac.remove(&actor, snapshot);
                    player_ac.remove(&actor);
                    chunk_ticket_ac.remove(&actor);

                    model_acc.remove_actor(&actor, snapshot);
                }
            },
            ServerEvent::SharedEvent(event) => {
                match event {
                    SharedEvent::ChunkLoaded {
                        data: chunk_data,
                        data_encoded,
                    } => {
                        let chunk_data_buf = data_encoded.extract();
                        match status_cc.get_mut(&chunk_data.chunk) {
                            Some(status) if *status == ChunkStatus::Loading => {
                                *status = ChunkStatus::Active;
                            },
                            _ => continue,
                        }

                        class_bc.insert_chunk(chunk_data.chunk, chunk_data.block_classes);
                        cache_cc.insert(chunk_data.chunk, chunk_data_buf.clone());

                        let chunk = chunk_data.chunk;

                        for (player, client) in actor_pc.iter().filter_map(|(player, actor)| {
                            let chunk_ticket = chunk_ticket_ac.get(actor)?;
                            if chunk_ticket
                                .chunk
                                .radius(chunk_ticket.radius)
                                .is_within(&chunk)
                            {
                                Some((player, client_pc.get(player)?))
                            } else {
                                None
                            }
                        }) {
                            if client
                                .tx
                                .send(ClientEvent::SendDataReliable {
                                    channel: BASE_CHANNEL,
                                    data: SendData::Ref(chunk_data_buf.clone()),
                                })
                                .is_err()
                            {
                                let _ = local
                                    .event_tx
                                    .send(ServerEvent::RemovePlayer { player: *player });
                            }
                        }
                    },
                }
            },
            ServerEvent::ServerConnectionClosed => return,
        }
    }
}
