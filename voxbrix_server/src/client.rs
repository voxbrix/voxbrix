use crate::{
    entity::{
        actor::Actor,
        player::Player,
    },
    server::ServerEvent,
    storage::player::Player as PlayerStorage,
    Local,
    Shared,
    BASE_CHANNEL,
    CLIENT_CONNECTION_TIMEOUT,
    PLAYER_CHUNK_TICKET_RADIUS,
    PLAYER_TABLE,
    USERNAME_TABLE,
};
use async_io::Timer;
use futures_lite::{
    future::FutureExt,
    stream::{
        self,
        StreamExt,
    },
};
use log::warn;
use redb::ReadableTable;
use std::rc::Rc;
use voxbrix_common::{
    messages::{
        client::{
            InitFailure,
            InitResponse,
        },
        server::InitRequest,
    },
    pack::Pack,
    stream::StreamExt as _,
};
use voxbrix_protocol::{
    server::{
        StreamReceiver,
        StreamSender,
    },
    Channel,
    Packet,
};

// Client loop input
pub enum ClientEvent {
    AssignActor { actor: Actor },
    SendDataUnreliable { channel: Channel, data: SendData },
    SendDataReliable { channel: Channel, data: SendData },
}

enum SelfEvent {
    Exit,
}

enum LoopEvent {
    ServerLoop(ClientEvent),
    PeerMessage { channel: usize, data: Packet },
    SelfEvent(SelfEvent),
}

pub enum SendData {
    Owned(Vec<u8>),
    Ref(Rc<Vec<u8>>),
}

impl SendData {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Owned(v) => v.as_slice(),
            Self::Ref(v) => v.as_slice(),
        }
    }
}

pub async fn run(
    local: &'static Local,
    shared: &'static Shared,
    tx: StreamSender,
    mut rx: StreamReceiver,
) {
    let (mut unreliable_tx, mut reliable_tx) = tx.split();

    // Lookup for the player in the database,
    // if there's none - register,
    // if the password is not correct - send error
    let player_res = {
        match rx
            .recv()
            .await
            .ok()
            .and_then(|(_channel, data)| InitRequest::unpack(&data).ok())
        {
            Some(req) => {
                blocking::unblock(|| {
                    let InitRequest { username, password } = req;

                    let db_write = shared.database.begin_write().expect("database write");
                    let player = {
                        let mut player_table = db_write
                            .open_table(PLAYER_TABLE)
                            .expect("database table open");
                        let mut username_table = db_write
                            .open_table(USERNAME_TABLE)
                            .expect("database table open");

                        let player = match username_table.get(&username).expect("database read") {
                            Some(p) => p,
                            None => {
                                let player = player_table
                                    .iter()
                                    .expect("database read")
                                    .next_back()
                                    // TODO wrapping?
                                    .map(|(id, _)| id.checked_add(1).unwrap())
                                    .unwrap_or(0);

                                username_table
                                    .insert(&username, &player)
                                    .expect("database write");

                                player
                            },
                        };

                        match player_table
                            .get(&player)
                            .expect("database read")
                            .map(|bytes| PlayerStorage::unpack(bytes))
                            .transpose()
                            .map_err(|_| InitFailure::Unknown)?
                        {
                            Some(PlayerStorage {
                                username: st_un,
                                password: st_ps,
                            }) => {
                                if username == st_un && password == st_ps {
                                    Ok(player)
                                } else {
                                    Err(InitFailure::IncorrectPassword)
                                }
                            },
                            None => {
                                player_table
                                    .insert(
                                        &player,
                                        &PlayerStorage { username, password }.pack_to_vec(),
                                    )
                                    .expect("database write");

                                Ok(player)
                            },
                        }
                    }?;
                    db_write.commit().expect("database commit");

                    Ok(player)
                })
                .await
            },
            None => return,
        }
    };

    let player = match player_res {
        Ok(p) => Player(p),
        Err(err) => {
            reliable_tx
                .send_reliable(BASE_CHANNEL, &InitResponse::Failure(err).pack_to_vec())
                .await;
            return;
        },
    };

    let (client_tx, mut server_rx) = local_channel::mpsc::channel();
    let (self_tx, self_rx) = local_channel::mpsc::channel();

    let _ = local
        .event_tx
        .send(ServerEvent::AddPlayer { player, client_tx });

    let actor = match server_rx.recv().await {
        Some(ClientEvent::AssignActor { actor }) => actor,
        _ => panic!("client_loop: incorrect answer to AddPlayer"),
    };

    let (unreliable_loop_tx, mut unreliable_loop_rx) =
        local_channel::mpsc::channel::<(Channel, SendData)>();
    let self_tx_local = self_tx.clone();
    local
        .rt
        .spawn(async move {
            while let Some((channel, data)) = unreliable_loop_rx.recv().await {
                if let Err(err) = unreliable_tx
                    .send_unreliable(channel, data.as_slice())
                    .await
                {
                    warn!("client_loop: send_unreliable error {:?}", err);
                    let _ = self_tx_local.send(SelfEvent::Exit);
                    return;
                }
            }
        })
        .detach();

    let (reliable_loop_tx, mut reliable_loop_rx) =
        local_channel::mpsc::channel::<(Channel, SendData)>();
    local
        .rt
        .spawn(async move {
            while let Some((channel, data)) = reliable_loop_rx.recv().await {
                match (async { Ok(reliable_tx.send_reliable(channel, data.as_slice()).await) })
                    .or(async {
                        Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
                        Err(())
                    })
                    .await
                {
                    Err(_) => {
                        warn!("client_loop: send_reliable timeout {:?}", player);
                        let _ = self_tx.send(SelfEvent::Exit);
                        return;
                    },
                    Ok(Err(err)) => {
                        warn!("client_loop: send_reliable error {:?}", err);
                        let _ = self_tx.send(SelfEvent::Exit);
                        return;
                    },
                    Ok(Ok(())) => {},
                }
            }
        })
        .detach();

    // Finalize successful connection
    reliable_loop_tx.send((
        BASE_CHANNEL,
        SendData::Owned(
            InitResponse::Success {
                actor,
                player_ticket_radius: PLAYER_CHUNK_TICKET_RADIUS,
            }
            .pack_to_vec(),
        ),
    ));

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
            }))
            .or_ff(self_rx.map(|le| LoopEvent::SelfEvent(le))),
    );

    while let Some(event) = events.next().await {
        match event {
            LoopEvent::ServerLoop(event) => {
                match event {
                    ClientEvent::SendDataUnreliable { channel, data } => {
                        let _ = unreliable_loop_tx.send((channel, data));
                    },
                    ClientEvent::SendDataReliable { channel, data } => {
                        let _ = reliable_loop_tx.send((channel, data));
                    },
                    _ => {},
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
            LoopEvent::SelfEvent(event) => {
                match event {
                    SelfEvent::Exit => break,
                }
            },
        }
    }

    let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
}
