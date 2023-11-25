use crate::{
    entity::player::Player,
    storage::{
        player::PlayerProfile,
        Data,
        DataSized,
    },
};
use anyhow::Result;
use client_loop::ClientLoop;
use log::{
    error,
    warn,
};
use redb::{
    Database,
    TableDefinition,
};
use server_loop::{
    ServerEvent,
    ServerLoop,
};
use std::{
    env,
    sync::Arc,
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
const PLAYER_CHUNK_VIEW_RADIUS: i32 = 8;
const PROCESS_INTERVAL: Duration = Duration::from_millis(50);
const CLIENT_CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const BLOCK_CLASS_TABLE: TableDefinition<DataSized<Chunk>, Data<BlocksVec<BlockClass>>> =
    TableDefinition::new("block_class");
const PLAYER_TABLE: TableDefinition<DataSized<Player>, Data<PlayerProfile>> =
    TableDefinition::new("player");
const USERNAME_TABLE: TableDefinition<&str, DataSized<Player>> = TableDefinition::new("username");

mod assets;
mod client_loop;
mod component;
mod entity;
mod server_loop;
mod storage;
mod system;

fn main() -> Result<()> {
    env_logger::init();
    let database = Arc::new(Database::create("/tmp/voxbrix.db")?);

    let write_tx = database.begin_write()?;
    {
        // Initialize all tables
        write_tx.open_table(USERNAME_TABLE)?;
        write_tx.open_table(PLAYER_TABLE)?;
        write_tx.open_table(BLOCK_CLASS_TABLE)?;
    }
    write_tx.commit()?;

    let port = env::var("VOXBRIX_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12000);

    let rt = RuntimeBuilder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("unable to build runtime");

    rt.block_on(LocalSet::new().run_until(async move {
        let (event_tx, event_rx) = local_channel::mpsc::channel();

        {
            let server = ServerParameters::default()
                .bind(([0, 0, 0, 0], port))
                .await?;

            let database = database.clone();
            let event_tx = event_tx.clone();

            task::spawn_local(async move {
                let mut server = server;
                loop {
                    match server.accept().await {
                        Ok(connection) => {
                            let database = database.clone();
                            let event_tx = event_tx.clone();

                            task::spawn_local(async move {
                                let result = ClientLoop {
                                    database,
                                    event_tx,
                                    connection,
                                }
                                .run()
                                .await;

                                match result {
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
                            let _ = event_tx.send(ServerEvent::ServerConnectionClosed);
                        },
                    }
                }
            });
        }

        ServerLoop { database, event_rx }.run().await;

        Ok(())
    }))
}
