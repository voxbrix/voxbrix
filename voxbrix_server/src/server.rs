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
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
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
    entity::player::Player,
    storage::{
        StorageThread,
        Store,
        StoreSized,
    },
    system::chunk_ticket::ChunkTicketSystem,
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
    component::block::{
        class::ClassBlockComponent,
        BlocksVec,
    },
    entity::{
        actor_class::ActorClass,
        block::{
            BLOCKS_IN_CHUNK,
            BLOCKS_IN_CHUNK_LAYER,
        },
    },
    messages::{
        client::{
            ActorStatus,
            ClientAccept,
        },
        server::ServerAccept,
    },
    pack::PackZip,
    system::block_class_loading::BlockClassLoadingSystem,
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
}

/// Packs data into Rc in one thread and extrachunk_ticket_system it in another
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
    let block_class_loading_system = BlockClassLoadingSystem::load_data()
        .await
        .expect("loading block classes");
    let block_class_label_map = block_class_loading_system.into_label_map();

    let time_start = Instant::now();

    let mut client_pc = ClientPlayerComponent::new();
    let mut actor_pc = ActorPlayerComponent::new();
    let mut class_ac = ClassActorComponent::new();
    let mut chunk_ticket_ac = ChunkTicketActorComponent::new();
    let mut status_cc = StatusChunkComponent::new();
    let mut cache_cc = CacheChunkComponent::new();
    let mut position_ac = PositionActorComponent::new();
    let mut velocity_ac = VelocityActorComponent::new();
    let mut orientation_ac = OrientationActorComponent::new();
    let mut chunk_ticket_system = ChunkTicketSystem::new();
    let mut class_bc = ClassBlockComponent::new();

    // TODO classify actors to know what to send without a buffer
    let mut status_updates = std::collections::BTreeSet::new();

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

    while let Some(event) = stream.next().await {
        match event {
            ServerEvent::Process => {
                let timestamp = time_start.elapsed();

                chunk_ticket_system.clear();
                chunk_ticket_system.actor_tickets(&chunk_ticket_ac);

                for (player, client, position) in actor_pc.iter().filter_map(|(player, actor)| {
                    Some((player, client_pc.get(player)?, position_ac.get(actor)?))
                }) {
                    let chunk_radius = position.chunk.radius(PLAYER_CHUNK_TICKET_RADIUS);

                    let status = class_ac
                        .iter()
                        .filter_map(|(actor, &class)| {
                            let position = position_ac.get(&actor)?.clone();
                            let orientation = orientation_ac.get(&actor)?.clone();

                            if !chunk_radius.is_within(&position.chunk) {
                                return None;
                            }

                            let velocity = velocity_ac.get(&actor)?.clone();

                            Some(ActorStatus {
                                actor,
                                class,
                                position,
                                velocity,
                                orientation,
                            })
                        })
                        .collect();

                    let data = ClientAccept::ActorStatus { timestamp, status }.pack_to_vec();

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
                        let chunk_key = chunk.store_sized();

                        let block_classes = {
                            let db_read = shared.database.begin_read().unwrap();
                            let table = db_read
                                .open_table(BLOCK_CLASS_TABLE)
                                .expect("server_loop: database read");
                            table
                                .get(&chunk_key)
                                .unwrap()
                                .and_then(|bytes| bytes.value().unstore().ok())
                        }
                        .unwrap_or_else(|| {
                            let block_classes = if chunk.position[2] == -1 {
                                let mut chunk_blocks = vec![stone; BLOCKS_IN_CHUNK];
                                for block_class in (&mut chunk_blocks
                                    [BLOCKS_IN_CHUNK - BLOCKS_IN_CHUNK_LAYER .. BLOCKS_IN_CHUNK])
                                    .iter_mut()
                                {
                                    *block_class = grass;
                                }
                                BlocksVec::new(chunk_blocks)
                            } else if chunk.position[2] < -1 {
                                BlocksVec::new(vec![stone; BLOCKS_IN_CHUNK])
                            } else {
                                BlocksVec::new(vec![air; BLOCKS_IN_CHUNK])
                            };

                            let db_write = shared.database.begin_write().unwrap();
                            {
                                let mut table = db_write.open_table(BLOCK_CLASS_TABLE).unwrap();
                                table
                                    .insert(&chunk_key, block_classes.store_owned())
                                    .expect("server_loop: database write");
                            }
                            db_write.commit().unwrap();

                            block_classes
                        });

                        let data = ChunkData {
                            chunk: *chunk,
                            block_classes,
                        };

                        // Moving allocations and cloning away from the main thread
                        let data_encoded =
                            SendRc::new(ClientAccept::ChunkData(data.clone()).pack_to_vec());

                        let _ = shared
                            .event_tx
                            .send(SharedEvent::ChunkLoaded { data, data_encoded });
                    },
                );
            },
            ServerEvent::AddPlayer {
                player,
                client_tx: tx,
            } => {
                let tx_init = tx.clone();
                let actor = class_ac.create_actor(ActorClass(0));
                client_pc.insert(player, Client { tx });
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
                    let event = match ServerAccept::unpack(data) {
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
                        ServerAccept::PlayerMovement {
                            position,
                            velocity,
                            orientation,
                        } => {
                            let actor = match actor_pc.get(&player) {
                                Some(a) => a,
                                None => continue,
                            };

                            let chunk = position.chunk;

                            let curr_pos = position_ac.insert(*actor, position);
                            velocity_ac.insert(*actor, velocity);

                            orientation_ac.insert(*actor, orientation);

                            status_updates.insert(*actor);

                            if curr_pos.is_none()
                                || curr_pos.is_some() && curr_pos.unwrap().chunk != chunk
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

                                for chunk_data in curr_radius.into_iter().filter_map(|chunk| {
                                    if let Some(prev_radius) = &prev_radius {
                                        if prev_radius.is_within(&chunk) {
                                            return None;
                                        }
                                    }

                                    cache_cc.get(&chunk)
                                }) {
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

                                let data_buf = Rc::new(
                                    ClientAccept::AlterBlock {
                                        chunk,
                                        block,
                                        block_class,
                                    }
                                    .pack_to_vec(),
                                );

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

                                cache_cc.insert(chunk, Rc::new(cache_data.pack_to_vec()));

                                let blocks_cache = match cache_data {
                                    ClientAccept::ChunkData(b) => b.block_classes,
                                    _ => panic!(),
                                };

                                storage.execute(move |buf| {
                                    let db_write = shared.database.begin_write().unwrap();
                                    {
                                        let mut table =
                                            db_write.open_table(BLOCK_CLASS_TABLE).unwrap();

                                        table
                                            .insert(chunk.store_sized(), blocks_cache.store(buf))
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
                    class_ac.remove(&actor);
                    position_ac.remove(&actor);
                    chunk_ticket_ac.remove(&actor);
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
