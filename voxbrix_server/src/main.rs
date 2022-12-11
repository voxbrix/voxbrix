use crate::{
    component::{
        actor::{
            chunk_ticket::{
                ActorChunkTicket,
                ChunkTicketActorComponent,
            },
            position::{
                GlobalPosition,
                GlobalPositionActorComponent,
            },
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
        actor::Actor,
        block::BLOCKS_IN_CHUNK,
        block_class::BlockClass,
        chunk::Chunk,
        player::Player,
    },
};
use anyhow::{
    Error,
    Result,
};
use async_executor::LocalExecutor;
use async_io::Timer;
use flume::{
    Receiver as SharedReceiver,
    Sender as SharedSender,
};
use futures_lite::{
    future::{
        self,
        FutureExt,
    },
    stream::{
        self,
        StreamExt,
    },
};
use local_channel::mpsc::{
    Receiver,
    Sender,
};
use log::{
    debug,
    error,
    warn,
};
use sled::{
    Batch,
    Db,
    Subscriber as DataSubscriber,
    Tree,
};
use std::{
    rc::Rc,
    time::Duration,
};
use system::chunk_ticket::ChunkTicketSystem;
use voxbrix_common::{
    messages::{
        client::{
            ClientAccept,
            ServerSettings,
        },
        server::ServerAccept,
    },
    pack::Pack,
    stream::StreamExt as _,
    ChunkData,
};
use voxbrix_protocol::{
    server::{
        Server,
        StreamReceiver,
        StreamSender,
    },
    Channel,
    Packet,
};

const PLAYER_CHUNK_TICKET_RADIUS: i32 = 2;
const PROCESS_INTERVAL: Duration = Duration::from_secs(1);
const CLIENT_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

mod component;
mod entity;
mod store;
mod system;

struct Local {
    rt: LocalExecutor<'static>,
    event_tx: Sender<ServerEvent>,
}

struct Shared {
    database: Db,
    event_tx: SharedSender<SharedEvent>,
}

// Client loop input
enum ClientEvent {
    SendDataRef { channel: Channel, data: Rc<Vec<u8>> },
}

enum SharedEvent {
    ChunkLoaded(ChunkData),
}

// Server loop input
enum ServerEvent {
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

struct Client {
    tx: Sender<ClientEvent>,
}

const BASE_CHANNEL: Channel = 0;

async fn server_loop(
    local: &'static Local,
    shared: &'static Shared,
    event_rx: Receiver<ServerEvent>,
    event_shared_rx: SharedReceiver<SharedEvent>,
) {
    // TODO remake in ECS
    let mut cpc = ClientPlayerComponent::new();
    let mut apc = ActorPlayerComponent::new();
    let mut ctac = ChunkTicketActorComponent::new();
    let mut scc = StatusChunkComponent::new();
    let mut gpac = GlobalPositionActorComponent::new();
    let mut cts = ChunkTicketSystem::new();
    let mut cbc = ClassBlockComponent::new();

    let mut stream = Timer::interval(PROCESS_INTERVAL)
        .map(|_| ServerEvent::Process)
        .or(event_rx)
        .or(event_shared_rx.stream().map(ServerEvent::SharedEvent));

    while let Some(event) = stream.next().await {
        match event {
            ServerEvent::Process => {
                cts.clear();
                cts.actor_tickets(&ctac);
                cts.apply(&mut scc, &mut cbc, |chunk| {
                    let response = if chunk.position[2] < -1 {
                        ChunkData {
                            chunk: *chunk,
                            block_classes: Blocks::new([BlockClass(1); BLOCKS_IN_CHUNK]),
                        }
                    } else {
                        ChunkData {
                            chunk: *chunk,
                            block_classes: Blocks::new([BlockClass(0); BLOCKS_IN_CHUNK]),
                        }
                    };

                    let _ = shared.event_tx.send(SharedEvent::ChunkLoaded(response));
                });
            },
            ServerEvent::AddPlayer {
                player,
                client_tx: tx,
            } => {
                cpc.insert(player, Client { tx });
                apc.insert(player, Actor(0));
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
                    }
                }
            },
            ServerEvent::RemovePlayer { player } => {
                cpc.remove(&player);
                if let Some(actor) = apc.remove(&player) {
                    gpac.remove(&actor);
                    ctac.remove(&actor);
                }
            },
            ServerEvent::SharedEvent(event) => {
                match event {
                    SharedEvent::ChunkLoaded(chunk_data) => {
                        match scc.get_mut(&chunk_data.chunk) {
                            Some(status) if *status == ChunkStatus::Loading => {
                                *status = ChunkStatus::Active;
                            },
                            _ => continue,
                        }

                        cbc.insert_chunk(chunk_data.chunk, chunk_data.block_classes.clone());

                        let chunk = chunk_data.chunk;

                        let mut chunk_data_buf = Vec::new();
                        ClientAccept::ChunkData(chunk_data)
                            .pack(&mut chunk_data_buf)
                            .expect("chunk data pack");

                        let chunk_data_buf = Rc::new(chunk_data_buf);

                        for (player, client) in apc.iter().filter_map(|(player, actor)| {
                            let chunk_ticket = ctac.get(actor)?;
                            if chunk.dimension == chunk_ticket.chunk.dimension
                                && chunk.position[0]
                                    >= chunk_ticket.chunk.position[0]
                                        .saturating_sub(PLAYER_CHUNK_TICKET_RADIUS)
                                && chunk.position[0]
                                    <= chunk_ticket.chunk.position[0]
                                        .saturating_add(PLAYER_CHUNK_TICKET_RADIUS)
                                && chunk.position[1]
                                    >= chunk_ticket.chunk.position[1]
                                        .saturating_sub(PLAYER_CHUNK_TICKET_RADIUS)
                                && chunk.position[1]
                                    <= chunk_ticket.chunk.position[1]
                                        .saturating_add(PLAYER_CHUNK_TICKET_RADIUS)
                                && chunk.position[2]
                                    >= chunk_ticket.chunk.position[2]
                                        .saturating_sub(PLAYER_CHUNK_TICKET_RADIUS)
                                && chunk.position[2]
                                    <= chunk_ticket.chunk.position[2]
                                        .saturating_add(PLAYER_CHUNK_TICKET_RADIUS)
                            {
                                Some((player, cpc.get(player)?))
                            } else {
                                None
                            }
                        }) {
                            if let Err(_) = client.tx.send(ClientEvent::SendDataRef {
                                channel: BASE_CHANNEL,
                                data: chunk_data_buf.clone(),
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

async fn client_loop(
    local: &'static Local,
    shared: &'static Shared,
    mut tx: StreamSender,
    rx: StreamReceiver,
) {
    enum LoopEvent {
        ServerLoop(ClientEvent),
        PeerMessage { channel: usize, data: Packet },
    }

    let (client_tx, server_rx) = local_channel::mpsc::channel();

    let player = Player(0);

    let _ = local
        .event_tx
        .send(ServerEvent::AddPlayer { player, client_tx });

    macro_rules! send_reliable {
        ($channel:expr, $buffer:expr) => {
            match (async { Ok(tx.send_reliable($channel, $buffer).await) })
                .or(async {
                    Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
                    Err(())
                })
                .await
            {
                Err(_) => {
                    warn!("client_loop: send_reliable timeout {:?}", player);
                    let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
                    return;
                },
                Ok(Err(err)) => {
                    warn!("client_loop: send_reliable error {:?}", err);
                    let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
                    return;
                },
                Ok(Ok(())) => {},
            }
        };
    }

    let mut buffer = Vec::new();

    ServerSettings {
        player_ticket_radius: PLAYER_CHUNK_TICKET_RADIUS as u8,
    }
    .pack(&mut buffer)
    .expect("pack server_settings");

    send_reliable!(BASE_CHANNEL, &buffer);

    let mut events = Box::pin(
        server_rx
            .map(|le| LoopEvent::ServerLoop(le))
            .or_ff(stream::unfold(rx, |mut rx| {
                (async move {
                    match rx.recv().await {
                        Ok((channel, data)) => Some((LoopEvent::PeerMessage { channel, data }, rx)),
                        Err(err) => {
                            warn!("client_loop: connection interrupted: {:?}", err);
                            None
                        },
                    }
                })
                .or(async {
                    Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
                    warn!("client_loop: connection timeout");
                    None
                })
            })),
    );

    while let Some(event) = events.next().await {
        match event {
            LoopEvent::ServerLoop(event) => {
                match event {
                    ClientEvent::SendDataRef { channel, data } => {
                        send_reliable!(channel, data.as_ref());
                    },
                }
            },
            LoopEvent::PeerMessage { channel, data } => {
                if let Err(_) = local.event_tx.send(ServerEvent::PlayerEvent {
                    player,
                    channel,
                    data,
                }) {
                    break;
                }
            },
        }
    }

    let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
}

fn main() -> Result<()> {
    env_logger::init();
    let database = sled::open("/tmp/voxbrix.db")?;

    let (event_tx, event_shared_rx) = flume::unbounded();

    let shared = Box::leak(Box::new(Shared { database, event_tx }));

    let (event_tx, event_rx) = local_channel::mpsc::channel();

    let local = Box::leak(Box::new(Local {
        rt: LocalExecutor::new(),
        event_tx,
    }));

    let server = Server::bind(([127, 0, 0, 1], 12000))?;

    future::block_on(local.rt.run(async {
        local
            .rt
            .spawn(async {
                let mut server = server;
                loop {
                    match server.accept().await {
                        Ok((tx, rx)) => {
                            local.rt.spawn(client_loop(local, shared, tx, rx)).detach();
                        },
                        Err(err) => {
                            error!("main: server.accept() error: {:?}", err);
                            let _ = local.event_tx.send(ServerEvent::ServerConnectionClosed);
                        },
                    }
                }
            })
            .detach();

        server_loop(local, shared, event_rx, event_shared_rx).await;
    }));

    Ok(())
}
