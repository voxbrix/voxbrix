use crate::{
    entity::player::Player,
    server::ServerEvent,
    Local,
    Shared,
    BASE_CHANNEL,
    CLIENT_CONNECTION_TIMEOUT,
    PLAYER_CHUNK_TICKET_RADIUS,
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
use std::rc::Rc;
use voxbrix_common::{
    messages::client::ServerSettings,
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
    SendDataRefUnreliable { channel: Channel, data: Rc<Vec<u8>> },
    SendDataRefReliable { channel: Channel, data: Rc<Vec<u8>> },
}

pub async fn run(
    local: &'static Local,
    shared: &'static Shared,
    tx: StreamSender,
    rx: StreamReceiver,
) {
    let (mut unreliable_tx, mut reliable_tx) = tx.split();

    enum SelfEvent {
        Exit,
    }

    enum LoopEvent {
        ServerLoop(ClientEvent),
        PeerMessage { channel: usize, data: Packet },
        SelfEvent(SelfEvent),
    }

    let (client_tx, server_rx) = local_channel::mpsc::channel();
    let (self_tx, self_rx) = local_channel::mpsc::channel();

    let player = Player(0);

    let _ = local
        .event_tx
        .send(ServerEvent::AddPlayer { player, client_tx });

    let (unreliable_loop_tx, mut unreliable_loop_rx) =
        local_channel::mpsc::channel::<(Channel, Rc<Vec<u8>>)>();
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
        local_channel::mpsc::channel::<(Channel, Rc<Vec<u8>>)>();
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

    reliable_loop_tx.send((
        BASE_CHANNEL,
        Rc::new(
            ServerSettings {
                player_ticket_radius: PLAYER_CHUNK_TICKET_RADIUS as u8,
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
                    ClientEvent::SendDataRefUnreliable { channel, data } => {
                        let _ = unreliable_loop_tx.send((channel, data));
                    },
                    ClientEvent::SendDataRefReliable { channel, data } => {
                        let _ = reliable_loop_tx.send((channel, data));
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
            LoopEvent::SelfEvent(event) => {
                match event {
                    SelfEvent::Exit => break,
                }
            },
        }
    }

    let _ = local.event_tx.send(ServerEvent::RemovePlayer { player });
}
