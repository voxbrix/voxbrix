use crate::{
    component::{
        actor::{
            chunk_ticket::{
                ActorChunkTicket,
                ChunkTicketActorComponent,
            },
            position::GlobalPositionActorComponent,
        },
        block::{
            class::ClassBlockComponent,
            coords_iter,
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
        block::{
            self,
            BLOCKS_IN_CHUNK,
        },
        block_class::BlockClass,
        chunk,
        player::Player,
    },
    server::ServerEvent,
    store::AsKey,
    Local,
    Shared,
    BASE_CHANNEL,
    CLIENT_CONNECTION_TIMEOUT,
    PLAYER_CHUNK_TICKET_RADIUS,
};
use anyhow::Result;
use arrayvec::ArrayVec;
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
};
use std::{
    rc::Rc,
    time::Duration,
};
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

// Client loop input
pub enum ClientEvent {
    SendDataRef { channel: Channel, data: Rc<Vec<u8>> },
}

pub async fn run(
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

    send_reliable!(
        BASE_CHANNEL,
        &ServerSettings {
            player_ticket_radius: PLAYER_CHUNK_TICKET_RADIUS as u8,
        }
        .pack_to_vec()
    );

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
