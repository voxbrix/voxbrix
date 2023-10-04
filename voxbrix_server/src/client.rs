use crate::{
    component::player::client::{
        ClientEvent,
        SendData,
    },
    entity::player::Player,
    server::ServerEvent,
    storage::{
        player::PlayerProfile,
        IntoData,
        IntoDataSized,
    },
    Local,
    Shared,
    BASE_CHANNEL,
    CLIENT_CONNECTION_TIMEOUT,
    PLAYER_CHUNK_VIEW_RADIUS,
    PLAYER_TABLE,
    USERNAME_TABLE,
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
use k256::ecdsa::{
    signature::{
        Signer,
        Verifier,
    },
    Signature,
    SigningKey,
    VerifyingKey,
};
use log::warn;
use redb::ReadableTable;
use tokio::{
    task,
    time,
};
use voxbrix_common::{
    async_ext::StreamExt as _,
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
    pack::Packer,
};
use voxbrix_protocol::{
    server::{
        Connection,
        Packet,
    },
    Channel,
};

enum LoopEvent {
    ServerLoop(ClientEvent),
    PeerMessage { channel: usize, data: Packet },
    Exit,
}

#[derive(Debug)]
pub enum Error {
    UnexpectedMessage,
    InitializationTimeout,
    Io,
    FailedRegistration,
    FailedLogin,
    ReceiverClosed,
    SenderClosed,
    ReliableSendTimeout,
    ReceiveTimeout,
    ReliableSendError,
    UnreliableSendError,
    ReceiveError,
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

    let mut packer = Packer::new();

    // Lookup for the player in the database,
    // if there's none - register,
    // if the password is not correct - send error
    let request = time::timeout(CLIENT_CONNECTION_TIMEOUT, async {
        rx.recv().await.error(Error::ReceiverClosed)
    })
    .await
    .map_err(|_| Error::InitializationTimeout)?
    .and_then(|(_channel, data)| {
        packer
            .unpack::<InitRequest>(data.as_ref())
            .error(Error::UnexpectedMessage)
    })?;

    // TODO: read from config
    let private_key = SigningKey::from_bytes((&[3; 32]).into()).unwrap();
    let public_key = private_key
        .verifying_key()
        .to_encoded_point(true)
        .as_bytes()
        .try_into()
        .unwrap();

    let key_signature: Signature = private_key.sign(&self_key);

    packer.pack(
        &InitResponse {
            public_key,
            key_signature: key_signature.to_bytes().into(),
        },
        &mut buffer,
    );

    time::timeout(CLIENT_CONNECTION_TIMEOUT, async {
        reliable_tx
            .send_reliable(BASE_CHANNEL, &buffer)
            .await
            .error(Error::Io)
    })
    .await
    .map_err(|_| Error::InitializationTimeout)??;

    let player = match request {
        InitRequest::Login => {
            let LoginRequest {
                username,
                key_signature,
            } = time::timeout(CLIENT_CONNECTION_TIMEOUT, async {
                rx.recv().await.error(Error::ReceiverClosed)
            })
            .await
            .map_err(|_| Error::InitializationTimeout)?
            .and_then(|(_channel, data)| {
                packer
                    .unpack::<LoginRequest>(data.as_ref())
                    .error(Error::UnexpectedMessage)
            })?;

            let player_res = task::spawn_blocking(move || {
                let mut packer = Packer::new();

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
                    .map(|bytes| bytes.value().into_inner())
                    .ok_or(LoginFailure::IncorrectCredentials)?;

                let player = player_table
                    .get(player_id.into_data_sized())
                    .expect("database read")
                    .map(|bytes| bytes.value().into_inner(&mut packer))
                    .ok_or(LoginFailure::IncorrectCredentials)?;

                let public_key =
                    VerifyingKey::from_sec1_bytes(&player.public_key).map_err(|_| {
                        warn!("client login: unable to parse client key in the database");
                        LoginFailure::IncorrectCredentials
                    })?;

                let signature = Signature::from_bytes((&key_signature).into()).map_err(|_| {
                    warn!("client login: incorrect key signature format");
                    LoginFailure::IncorrectCredentials
                })?;

                public_key
                    .verify(&peer_key, &signature)
                    .map_err(|_| LoginFailure::IncorrectCredentials)?;

                Ok(player_id)
            })
            .await
            .unwrap();

            match player_res {
                Ok(p) => p,
                Err(failure) => {
                    packer.pack(&LoginResult::Failure(failure), &mut buffer);
                    let _ = time::timeout(CLIENT_CONNECTION_TIMEOUT, async {
                        reliable_tx
                            .send_reliable(BASE_CHANNEL, &buffer)
                            .await
                            .error(Error::Io)
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
            } = time::timeout(CLIENT_CONNECTION_TIMEOUT, async {
                rx.recv().await.error(Error::ReceiverClosed)
            })
            .await
            .map_err(|_| Error::InitializationTimeout)?
            .and_then(|(_channel, data)| {
                packer
                    .unpack::<RegisterRequest>(data.as_ref())
                    .error(Error::UnexpectedMessage)
            })?;

            let player_res = task::spawn_blocking(move || {
                let mut packer = Packer::new();

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
                        .map(|(data, _)| {
                            let player = data.value().into_inner();
                            // TODO: some kind of wrapping?
                            Player(player.0.checked_add(1).unwrap())
                        })
                        .unwrap_or(Player(0));

                    username_table
                        .insert(username.as_str(), player.into_data_sized())
                        .expect("database write");

                    player_table
                        .insert(
                            player.into_data_sized(),
                            PlayerProfile {
                                username,
                                public_key,
                            }
                            .into_data(&mut packer),
                        )
                        .expect("database write");

                    player
                };
                db_write.commit().expect("database commit");

                Ok(player)
            })
            .await
            .unwrap();

            match player_res {
                Ok(p) => p,
                Err(failure) => {
                    packer.pack(&RegisterResult::Failure(failure), &mut buffer);
                    let _ = time::timeout(CLIENT_CONNECTION_TIMEOUT, async {
                        reliable_tx
                            .send_reliable(BASE_CHANNEL, &buffer)
                            .await
                            .error(Error::Io)
                    })
                    .await
                    .map_err(|_| Error::InitializationTimeout)?;

                    return Err(Error::FailedRegistration);
                },
            }
        },
    };

    let (client_tx, mut server_rx) = local_channel::mpsc::channel();

    let _ = local
        .event_tx
        .send(ServerEvent::AddPlayer { player, client_tx });

    let actor = match server_rx.recv().await {
        Some(ClientEvent::AssignActor { actor }) => actor,
        _ => panic!("client_loop: incorrect answer to AddPlayer"),
    };

    let (unreliable_loop_tx, mut unreliable_loop_rx) =
        local_channel::mpsc::channel::<(Channel, SendData)>();
    let unrel_send_task = stream::once_future(async move {
        while let Some((channel, data)) = unreliable_loop_rx.recv().await {
            if let Err(err) = unreliable_tx
                .send_unreliable(channel, data.as_slice())
                .await
            {
                warn!("client_loop: send_unreliable error {:?}", err);
                return Err(Error::UnreliableSendError);
            }
        }

        Ok(LoopEvent::Exit)
    });

    let (reliable_loop_tx, mut reliable_loop_rx) =
        local_channel::mpsc::channel::<(Channel, SendData)>();
    let rel_send_task = stream::once_future(async move {
        while let Some((channel, data)) = reliable_loop_rx
            .recv()
            .or(async {
                let _ = future::zip(reliable_tx.wait_complete(), future::pending::<()>()).await;
                unreachable!();
            })
            .await
        {
            match time::timeout(
                CLIENT_CONNECTION_TIMEOUT,
                reliable_tx.send_reliable(channel, data.as_slice()),
            )
            .await
            .map_err(|_| ())
            {
                Err(_) => {
                    warn!("client_loop: send_reliable timeout {:?}", player);
                    return Err(Error::ReliableSendTimeout);
                },
                Ok(Err(err)) => {
                    warn!("client_loop: send_reliable error {:?}", err);
                    return Err(Error::ReliableSendError);
                },
                Ok(Ok(())) => {},
            }
        }

        Ok(LoopEvent::Exit)
    });

    let init_data_response = match request {
        InitRequest::Login => {
            packer.pack_to_vec(&LoginResult::Success(InitData {
                actor,
                player_chunk_view_radius: PLAYER_CHUNK_VIEW_RADIUS,
            }))
        },
        InitRequest::Register => {
            packer.pack_to_vec(&RegisterResult::Success(InitData {
                actor,
                player_chunk_view_radius: PLAYER_CHUNK_VIEW_RADIUS,
            }))
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

    let recv_stream = stream::unfold(&mut rx, |rx| {
        async move {
            let nested_result = time::timeout(CLIENT_CONNECTION_TIMEOUT, async {
                rx.recv()
                    .await
                    .map(|(channel, data)| LoopEvent::PeerMessage { channel, data })
                    .map_err(|err| {
                        warn!("client_loop: connection interrupted: {:?}", err);
                        Error::ReceiveError
                    })
            })
            .await
            .map_err(|_| {
                warn!("client_loop: receive timeout");
                Error::ReceiveTimeout
            });

            let next = match nested_result {
                Ok(Ok(next)) => Ok(next),
                Ok(Err(err)) => Err(err),
                Err(err) => Err(err),
            };

            if next.is_err() {
                // we need to inform the server loop
                let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
            }

            Some((next, rx))
        }
    });

    let mut events = Box::pin(
        server_rx
            .map(|e| Ok(LoopEvent::ServerLoop(e)))
            .rr_ff(recv_stream)
            .rr_ff(rel_send_task)
            .rr_ff(unrel_send_task),
    );

    while let Some(event) = events.next().await {
        match event? {
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
            LoopEvent::Exit => break,
        }
    }

    Ok(())
}
