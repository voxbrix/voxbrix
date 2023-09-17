use crate::{
    entity::player::Player,
    storage::{
        player::PlayerProfile,
        Data,
        DataSized,
    },
};
use anyhow::Result;
use flume::Sender as SharedSender;
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
use std::{
    env,
    time::Duration,
};
use tokio::{
    runtime::Builder as RuntimeBuilder,
    task::{
        self,
        LocalSet,
    },
};
use voxbrix_common::{
    component::block::BlocksVec,
    entity::{
        block_class::BlockClass,
        chunk::Chunk,
    },
};
use voxbrix_protocol::{
    server::ServerParameters,
    Channel,
};

const BASE_CHANNEL: Channel = 0;
const PLAYER_CHUNK_TICKET_RADIUS: i32 = 4;
const PROCESS_INTERVAL: Duration = Duration::from_millis(50);
const CLIENT_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const BLOCK_CLASS_TABLE: TableDefinition<DataSized<Chunk>, Data<BlocksVec<BlockClass>>> =
    TableDefinition::new("block_class");
const PLAYER_TABLE: TableDefinition<DataSized<Player>, Data<PlayerProfile>> =
    TableDefinition::new("player");
const USERNAME_TABLE: TableDefinition<&str, DataSized<Player>> = TableDefinition::new("username");

mod client;
mod component;
mod entity;
mod server;
mod storage;
mod system;

pub struct Local {
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
        write_tx.open_table(USERNAME_TABLE)?;
        write_tx.open_table(PLAYER_TABLE)?;
        write_tx.open_table(BLOCK_CLASS_TABLE)?;
    }
    write_tx.commit()?;

    let (event_tx, event_shared_rx) = flume::unbounded();

    let shared = Box::leak(Box::new(Shared { database, event_tx }));

    let (event_tx, event_rx) = local_channel::mpsc::channel();

    let local = Box::leak(Box::new(Local { event_tx }));

    let port = env::var("VOXBRIX_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12000);

    let rt = RuntimeBuilder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("unable to build runtime");

    rt.block_on(LocalSet::new().run_until(async {
        let server = ServerParameters::default()
            .bind(([0, 0, 0, 0], port))
            .await?;

        task::spawn_local(async {
            let mut server = server;
            loop {
                match server.accept().await {
                    Ok(connection) => {
                        task::spawn_local(async {
                            match client::run(local, shared, connection).await {
                                Ok(_) => {
                                    warn!("client loop exited");
                                },
                                Err(err) => {
                                    warn!("client loop exited: {:?}", err);
                                },
                            }
                            // TODO send disconnect
                        });
                    },
                    Err(err) => {
                        error!("main: server.accept() error: {:?}", err);
                        let _ = local.event_tx.send(ServerEvent::ServerConnectionClosed);
                    },
                }
            }
        });

        server::run(local, shared, event_rx, event_shared_rx).await;

        Ok(())
    }))
}
