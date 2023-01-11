use anyhow::Result;
use async_executor::LocalExecutor;
use flume::Sender as SharedSender;
use futures_lite::future;
use local_channel::mpsc::Sender;
use log::{
    error,
    warn,
};
use redb::{
    Database,
    TableDefinition,
};
use server::{
    ServerEvent,
    SharedEvent,
};
use std::time::Duration;
use voxbrix_protocol::{
    server::ServerParameters,
    Channel,
};

const BASE_CHANNEL: Channel = 0;
const PLAYER_CHUNK_TICKET_RADIUS: i32 = 4;
const PROCESS_INTERVAL: Duration = Duration::from_millis(50);
const CLIENT_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const BLOCK_CLASS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("block_class");
const PLAYER_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("player");
const USERNAME_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("username");

mod client;
mod component;
mod entity;
mod server;
mod storage;
mod system;

pub struct Local {
    pub rt: LocalExecutor<'static>,
    pub event_tx: Sender<ServerEvent>,
}

pub struct Shared {
    pub database: Database,
    pub event_tx: SharedSender<SharedEvent>,
}

fn main() -> Result<()> {
    env_logger::init();
    let database = Database::create("/tmp/voxbrix.db")?;

    let write_tx = database.begin_write()?;
    {
        // Initialize all tables
        write_tx.open_table(BLOCK_CLASS_TABLE)?;
    }
    write_tx.commit()?;

    let (event_tx, event_shared_rx) = flume::unbounded();

    let shared = Box::leak(Box::new(Shared { database, event_tx }));

    let (event_tx, event_rx) = local_channel::mpsc::channel();

    let local = Box::leak(Box::new(Local {
        rt: LocalExecutor::new(),
        event_tx,
    }));

    let server = ServerParameters::default().bind(([0, 0, 0, 0], 12000))?;

    future::block_on(local.rt.run(async {
        local
            .rt
            .spawn(async {
                let mut server = server;
                loop {
                    match server.accept().await {
                        Ok(connection) => {
                            local
                                .rt
                                .spawn(async {
                                    match client::run(local, shared, connection).await {
                                        Ok(_) => {
                                            warn!("client loop exited");
                                        },
                                        Err(err) => {
                                            warn!("client loop exited: {:?}", err);
                                        },
                                    }
                                    // TODO send disconnect
                                })
                                .detach();
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
