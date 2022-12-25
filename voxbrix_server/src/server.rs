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
            existence::{
                ActorExistence,
                ExistenceActorComponent,
            },
            position::GlobalPositionActorComponent,
        },
        block::{
            class::ClassBlockComponent,
            Blocks,
        },
        chunk::status::{
            ChunkStatus,
            StatusChunkComponent,
        },
        player::{
            actor::ActorPlayerComponent,
            client::ClientPlayerComponent,
        },
    },
    entity::{
        block::{
            self,
            BLOCKS_IN_CHUNK,
        },
        block_class::BlockClass,
        chunk,
        player::Player,
    },
    store::{
        AsKey,
        StoreThread,
    },
    system::chunk_ticket::ChunkTicketSystem,
    Local,
    Shared,
    BASE_CHANNEL,
    BLOCK_CLASS_TABLE,
    PLAYER_CHUNK_TICKET_RADIUS,
    PROCESS_INTERVAL,
};
use arrayvec::ArrayVec;
use async_io::Timer;
use flume::Receiver as SharedReceiver;
use futures_lite::stream::StreamExt;
use local_channel::mpsc::{
    Receiver,
    Sender,
};
use log::debug;
use redb::ReadableTable;
use std::{
    rc::Rc,
    time::Duration,
};
use voxbrix_common::{
    messages::{
        client::ClientAccept,
        server::ServerAccept,
    },
    pack::Pack,
    ChunkData,
};
use voxbrix_protocol::{
    Channel,
    Packet,
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

/// Packs data into Rc in one thread and extracts it in another
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
    let mut cpc = ClientPlayerComponent::new();
    let mut apc = ActorPlayerComponent::new();
    let mut eac = ExistenceActorComponent::new();
    let mut ctac = ChunkTicketActorComponent::new();
    let mut scc = StatusChunkComponent::new();
    let mut gpac = GlobalPositionActorComponent::new();
    let mut cts = ChunkTicketSystem::new();
    let mut cbc = ClassBlockComponent::new();

    let mut stream = Timer::interval(PROCESS_INTERVAL)
        .map(|_| ServerEvent::Process)
        .or(event_rx)
        .or(event_shared_rx.stream().map(ServerEvent::SharedEvent));

    let store = StoreThread::new();

    while let Some(event) = stream.next().await {
        match event {
            ServerEvent::Process => {
                cts.clear();
                cts.actor_tickets(&ctac);
                cts.apply(&mut scc, &mut cbc, |chunk| {
                    let mut block_key = [0; block::KEY_LENGTH];
                    // TODO: some non-runtime way involving const generics?
                    let chunk_key = &mut block_key[.. chunk::KEY_LENGTH];
                    chunk.to_key(chunk_key);
                    let mut block_key_finish = block_key;
                    (&mut block_key_finish[block::KEY_LENGTH - 2 .. block::KEY_LENGTH])
                        .fill(u8::MAX);

                    let block_classes = {
                        let db_read = shared.database.begin_read().unwrap();
                        let table = db_read
                            .open_table(BLOCK_CLASS_TABLE)
                            .expect("server_loop: database read");
                        table
                            .range(block_key .. block_key_finish)
                            .unwrap()
                            .map(|(_key, bytes)| {
                                BlockClass::unpack(&bytes)
                                    .expect("server_loop: block class deserialization")
                            })
                            .collect::<ArrayVec<_, BLOCKS_IN_CHUNK>>()
                    };

                    let data = if block_classes.len() == BLOCKS_IN_CHUNK {
                        let block_classes = block_classes.into_inner().unwrap();
                        ChunkData {
                            chunk: *chunk,
                            block_classes: Blocks::new(block_classes),
                        }
                    } else {
                        let block_classes = if chunk.position[2] < -1 {
                            Blocks::new([BlockClass(1); BLOCKS_IN_CHUNK])
                        } else {
                            Blocks::new([BlockClass(0); BLOCKS_IN_CHUNK])
                        };

                        // TODO real chunk generation
                        std::thread::sleep(Duration::from_millis(500));

                        let db_write = shared.database.begin_write().unwrap();
                        {
                            let mut table = db_write.open_table(BLOCK_CLASS_TABLE).unwrap();
                            let mut val_buf = Vec::new();

                            for (block, block_class) in block_classes.iter() {
                                block.to_key(&mut block_key);
                                block_class.pack(&mut val_buf);

                                table
                                    .insert(&block_key, val_buf.as_slice())
                                    .expect("server_loop: database write");
                            }
                        }
                        db_write.commit().unwrap();

                        ChunkData {
                            chunk: *chunk,
                            block_classes,
                        }
                    };

                    // Moving allocations and cloning away from the main thread
                    let data_encoded =
                        SendRc::new(ClientAccept::ChunkData(data.clone()).pack_to_vec());

                    let _ = shared
                        .event_tx
                        .send(SharedEvent::ChunkLoaded { data, data_encoded });
                });
            },
            ServerEvent::AddPlayer {
                player,
                client_tx: tx,
            } => {
                let tx_init = tx.clone();
                let actor = eac.push(ActorExistence);
                cpc.insert(player, Client { tx });
                apc.insert(player, actor);

                tx_init.send(ClientEvent::AssignActor { actor });
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
                        ServerAccept::PlayerPosition { position } => {
                            let actor = match apc.get(&player) {
                                Some(a) => a,
                                None => continue,
                            };

                            let chunk = position.chunk;

                            let curr_pos = gpac.insert(*actor, position);

                            if curr_pos.is_none()
                                || curr_pos.is_some() && curr_pos.unwrap().chunk != chunk
                            {
                                ctac.insert(
                                    *actor,
                                    ActorChunkTicket {
                                        chunk,
                                        radius: PLAYER_CHUNK_TICKET_RADIUS,
                                    },
                                );
                                // TODO send all chunks that are already loaded but were out of range
                                // before
                            }
                        },
                        ServerAccept::AlterBlock {
                            chunk,
                            block,
                            block_class,
                        } => {
                            if let Some(block_class_ref) = cbc
                                .get_mut_chunk(&chunk)
                                .and_then(|blocks| blocks.get_mut(block))
                            {
                                *block_class_ref = block_class;

                                store.execute(move |buf| {
                                    let db_write = shared.database.begin_write().unwrap();
                                    {
                                        let mut table =
                                            db_write.open_table(BLOCK_CLASS_TABLE).unwrap();

                                        let mut block_key = [0; block::KEY_LENGTH];
                                        chunk.to_key(&mut block_key[.. chunk::KEY_LENGTH]);
                                        block.to_key(&mut block_key);
                                        block_class.pack(buf);

                                        table
                                            .insert(&block_key, buf.as_slice())
                                            .expect("server_loop: database write");
                                    }
                                    db_write.commit().unwrap();
                                });

                                let data_buf = Rc::new(
                                    ClientAccept::AlterBlock {
                                        chunk,
                                        block,
                                        block_class,
                                    }
                                    .pack_to_vec(),
                                );

                                for (player, client) in apc.iter().filter_map(|(player, actor)| {
                                    let ticket = ctac.get(actor)?;
                                    ticket
                                        .chunk
                                        .radius(ticket.radius)
                                        .is_within(&chunk)
                                        .then_some(())?;
                                    let client = cpc.get(player)?;
                                    Some((player, client))
                                }) {
                                    if let Err(_) = client.tx.send(ClientEvent::SendDataReliable {
                                        channel: BASE_CHANNEL,
                                        data: SendData::Ref(data_buf.clone()),
                                    }) {
                                        let _ = local
                                            .event_tx
                                            .send(ServerEvent::RemovePlayer { player: *player });
                                    }
                                }
                            }
                        },
                    }
                }
            },
            ServerEvent::RemovePlayer { player } => {
                cpc.remove(&player);
                if let Some(actor) = apc.remove(&player) {
                    eac.remove(&actor);
                    gpac.remove(&actor);
                    ctac.remove(&actor);
                }
            },
            ServerEvent::SharedEvent(event) => {
                match event {
                    SharedEvent::ChunkLoaded {
                        data: chunk_data,
                        data_encoded,
                    } => {
                        let chunk_data_buf = data_encoded.extract();
                        match scc.get_mut(&chunk_data.chunk) {
                            Some(status) if *status == ChunkStatus::Loading => {
                                *status = ChunkStatus::Active;
                            },
                            _ => continue,
                        }

                        cbc.insert_chunk(chunk_data.chunk, chunk_data.block_classes);

                        let chunk = chunk_data.chunk;

                        for (player, client) in apc.iter().filter_map(|(player, actor)| {
                            let chunk_ticket = ctac.get(actor)?;
                            if chunk_ticket
                                .chunk
                                .radius(chunk_ticket.radius)
                                .is_within(&chunk)
                            {
                                Some((player, cpc.get(player)?))
                            } else {
                                None
                            }
                        }) {
                            if let Err(_) = client.tx.send(ClientEvent::SendDataReliable {
                                channel: BASE_CHANNEL,
                                data: SendData::Ref(chunk_data_buf.clone()),
                            }) {
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
