use crate::{
    entity::{
        actor::Actor,
        player::Player,
    },
    server::ServerEvent,
    storage::{
        player::PlayerProfile,
        Store,
        StoreSized,
    },
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
use k256::ecdsa::{
    signature::{
        Signature as _,
        Signer,
        Verifier,
    },
    Signature,
    SigningKey,
    VerifyingKey,
};
use log::warn;
use redb::ReadableTable;
use std::rc::Rc;
use voxbrix_common::{
    messages::{
        client::{
            InitData,
            InitResponse,
            LoginFailure,
            LoginResult,
            RegisterFailure,
            RegisterResult,
        },
        server::{
            InitRequest,
            LoginRequest,
            RegisterRequest,
        },
    },
    pack::Pack,
    stream::StreamExt as _,
};
use voxbrix_protocol::{
    server::{
        Connection,
        Packet,
    },
    Channel,
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

#[derive(Debug)]
pub enum Error {
    UnexpectedMessage,
    Timeout,
    Io,
    FailedRegistration,
    FailedLogin,
    ReceiverClosed,
    SenderClosed,
}

trait ConvertResultError<T> {
    fn error(self, error: Error) -> Result<T, Error>;
}

impl<T, E> ConvertResultError<T> for Result<T, E> {
    fn error(self, error: Error) -> Result<T, Error> {
        self.map_err(|_| error)
    }
}

trait ConvertOption<T> {
    fn error(self, error: Error) -> Result<T, Error>;
}

impl<T> ConvertResultError<T> for Option<T> {
    fn error(self, error: Error) -> Result<T, Error> {
        self.ok_or(error)
    }
}

pub async fn run(
    local: &'static Local,
    shared: &'static Shared,
    connection: Connection,
) -> Result<(), Error> {
    let mut buffer = Vec::new();

    let Connection {
        self_key,
        peer_key,
        sender: tx,
        receiver: mut rx,
    } = connection;

    let (mut unreliable_tx, mut reliable_tx) = tx.split();

    // Lookup for the player in the database,
    // if there's none - register,
    // if the password is not correct - send error
    let request = async { rx.recv().await.error(Error::ReceiverClosed) }
        .or(async {
            Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
            Err(Error::Timeout)
        })
        .await
        .and_then(|(_channel, data)| InitRequest::unpack(data).error(Error::UnexpectedMessage))?;

    // TODO: read from config
    let private_key = SigningKey::from_bytes(&[3; 32]).unwrap();
    let public_key = private_key.verifying_key().to_bytes().into();

    let key_signature: Signature = private_key.sign(&self_key);

    InitResponse {
        public_key,
        key_signature: key_signature.as_bytes().try_into().unwrap(),
    }
    .pack(&mut buffer);

    async {
        reliable_tx
            .send_reliable(BASE_CHANNEL, &buffer)
            .await
            .error(Error::Io)
    }
    .or(async {
        Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
        Err(Error::Timeout)
    })
    .await?;

    let player = match request {
        InitRequest::Login => {
            let LoginRequest {
                username,
                key_signature,
            } = async { rx.recv().await.error(Error::ReceiverClosed) }
                .or(async {
                    Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
                    Err(Error::Timeout)
                })
                .await
                .and_then(|(_channel, data)| {
                    LoginRequest::unpack(data).error(Error::UnexpectedMessage)
                })?;

            let player_res = blocking::unblock(move || {
                let db_read = shared.database.begin_read().expect("database write");

                let username_table = db_read
                    .open_table(USERNAME_TABLE)
                    .expect("database table open");

                let player_table = db_read
                    .open_table(PLAYER_TABLE)
                    .expect("database table open");

                let player_id = username_table
                    .get(username.as_str())
                    .expect("database read")
                    .and_then(|bytes| bytes.value().unstore_sized().ok())
                    .ok_or(LoginFailure::IncorrectCredentials)?;

                let player = player_table
                    .get(player_id.store_sized())
                    .expect("database read")
                    .and_then(|bytes| bytes.value().unstore().ok())
                    .ok_or(LoginFailure::IncorrectCredentials)?;

                let public_key =
                    VerifyingKey::from_sec1_bytes(&player.public_key).map_err(|_| {
                        warn!("client login: unable to parse client key in the database");
                        LoginFailure::IncorrectCredentials
                    })?;

                let signature = Signature::from_bytes(&key_signature).map_err(|_| {
                    warn!("client login: incorrect key signature format");
                    LoginFailure::IncorrectCredentials
                })?;

                public_key
                    .verify(&peer_key, &signature)
                    .map_err(|_| LoginFailure::IncorrectCredentials)?;

                Ok(player_id)
            })
            .await;

            match player_res {
                Ok(p) => p,
                Err(failure) => {
                    LoginResult::Failure(failure).pack(&mut buffer);
                    let _ = async {
                        reliable_tx
                            .send_reliable(BASE_CHANNEL, &buffer)
                            .await
                            .error(Error::Io)
                    }
                    .or(async {
                        Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
                        Err(Error::Timeout)
                    })
                    .await;

                    return Err(Error::FailedLogin);
                },
            }
        },
        InitRequest::Register => {
            let RegisterRequest {
                username,
                public_key,
            } = async { rx.recv().await.error(Error::ReceiverClosed) }
                .or(async {
                    Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
                    Err(Error::Timeout)
                })
                .await
                .and_then(|(_channel, data)| {
                    RegisterRequest::unpack(data).error(Error::UnexpectedMessage)
                })?;

            let player_res = blocking::unblock(move || {
                let db_write = shared.database.begin_write().expect("database write");
                let player = {
                    let mut username_table = db_write
                        .open_table(USERNAME_TABLE)
                        .expect("database table open");

                    if username_table
                        .get(username.as_str())
                        .expect("database read")
                        .is_some()
                    {
                        return Err(RegisterFailure::UsernameTaken);
                    }

                    let mut player_table = db_write
                        .open_table(PLAYER_TABLE)
                        .expect("database table open");

                    let player = player_table
                        .iter()
                        .expect("database read")
                        .next_back()
                        // TODO wrapping?
                        .map(|(bytes, _)| {
                            let player = bytes.value().unstore_sized().unwrap();
                            // TODO: some kind of wrapping?
                            Player(player.0.checked_add(1).unwrap())
                        })
                        .unwrap_or(Player(0));

                    username_table
                        .insert(username.as_str(), player.store_sized())
                        .expect("database write");

                    player_table
                        .insert(
                            player.store_sized(),
                            PlayerProfile {
                                username,
                                public_key,
                            }
                            .store_owned(),
                        )
                        .expect("database write");

                    player
                };
                db_write.commit().expect("database commit");

                Ok(player)
            })
            .await;

            match player_res {
                Ok(p) => p,
                Err(failure) => {
                    RegisterResult::Failure(failure).pack(&mut buffer);
                    let _ = async {
                        reliable_tx
                            .send_reliable(BASE_CHANNEL, &buffer)
                            .await
                            .error(Error::Io)
                    }
                    .or(async {
                        Timer::after(CLIENT_CONNECTION_TIMEOUT).await;
                        Err(Error::Timeout)
                    })
                    .await;

                    return Err(Error::FailedRegistration);
                },
            }
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

    let init_data_response = match request {
        InitRequest::Login => {
            LoginResult::Success(InitData {
                actor,
                player_ticket_radius: PLAYER_CHUNK_TICKET_RADIUS,
            })
            .pack_to_vec()
        },
        InitRequest::Register => {
            RegisterResult::Success(InitData {
                actor,
                player_ticket_radius: PLAYER_CHUNK_TICKET_RADIUS,
            })
            .pack_to_vec()
        },
    };

    // Finalize successful connection
    if let Err(err) = reliable_loop_tx.send((BASE_CHANNEL, SendData::Owned(init_data_response))) {
        warn!(
            "client_loop: unable to send initialization response: {:?}",
            err
        );
        let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
        return Err(Error::SenderClosed);
    }

    let mut events = Box::pin(
        server_rx
            .map(LoopEvent::ServerLoop)
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
            .or_ff(self_rx.map(LoopEvent::SelfEvent)),
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
                // Server loop is down
                local
                    .event_tx
                    .send(ServerEvent::PlayerEvent {
                        player,
                        channel,
                        data,
                    })
                    .error(Error::SenderClosed)?;
            },
            LoopEvent::SelfEvent(event) => {
                match event {
                    SelfEvent::Exit => break,
                }
            },
        }
    }

    let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
    Ok(())
}
