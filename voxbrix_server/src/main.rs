use anyhow::Result;
use async_executor::LocalExecutor;
use flume::Sender as SharedSender;
use futures_lite::future;
use local_channel::mpsc::Sender;
use log::error;
use server::{
    ServerEvent,
    SharedEvent,
};
use sled::Db;
use std::time::Duration;
use voxbrix_protocol::{
    server::Server,
    Channel,
};

const BASE_CHANNEL: Channel = 0;
const PLAYER_CHUNK_TICKET_RADIUS: i32 = 2;
const PROCESS_INTERVAL: Duration = Duration::from_secs(1);
const CLIENT_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

mod client;
mod component;
mod entity;
mod server;
mod store;
mod system;

pub struct Local {
    pub rt: LocalExecutor<'static>,
    pub event_tx: Sender<ServerEvent>,
}

pub struct Shared {
    pub database: Db,
    pub event_tx: SharedSender<SharedEvent>,
}

fn main() -> Result<()> {
    env_logger::init();
    let database = sled::open("/tmp/database")?;

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
                            local.rt.spawn(client::run(local, shared, tx, rx)).detach();
                        },
                        Err(err) => {
                            error!("main: server.accept() error: {:?}", err);
                            let _ = local.event_tx.send(ServerEvent::ServerConnectionClosed);
                        },
                    }
                }
            })
            .detach();

        server::run(local, shared, event_rx, event_shared_rx).await;
    }));

    Ok(())
}
