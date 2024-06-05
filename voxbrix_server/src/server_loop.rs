use crate::{
    assets::{
        ACTION_LIST,
        ACTION_SCRIPT_MAP,
        DIMENSION_KIND_LIST,
        SERVER_LOOP_SCRIPT_DIR,
        SERVER_LOOP_SCRIPT_LIST,
    },
    component::{
        action::script::ScriptActionComponent,
        actor::{
            chunk_activation::ChunkActivationActorComponent,
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            player::PlayerActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::model::ModelActorClassComponent,
        block::class::ClassBlockComponent,
        chunk::{
            cache::CacheChunkComponent,
            status::StatusChunkComponent,
        },
        player::{
            actor::ActorPlayerComponent,
            chunk_update::ChunkUpdatePlayerComponent,
            chunk_view::ChunkViewPlayerComponent,
            client::{
                ClientEvent,
                ClientPlayerComponent,
            },
        },
    },
    entity::{
        actor::ActorRegistry,
        player::Player,
    },
    storage::StorageThread,
    system::{
        chunk_activation::ChunkActivationSystem,
        chunk_generation::ChunkGenerationSystem,
        map_loading::Map,
        position::PositionSystem,
    },
    BASE_CHANNEL,
    PROCESS_INTERVAL,
};
use data::{
    EntityRemoveQueue,
    SharedData,
};
use flume::Sender as SharedSender;
use futures_lite::stream::{
    self,
    StreamExt,
};
use local_channel::mpsc::Receiver;
use player_event::PlayerEvent;
use process::Process;
use redb::Database;
use std::{
    sync::Arc,
    time::Instant,
};
use tokio::{
    runtime::Handle,
    time::{
        self,
        MissedTickBehavior,
    },
};
use voxbrix_common::{
    assets::{
        ACTOR_MODEL_LIST_PATH,
        STATE_COMPONENTS_PATH,
    },
    component::block_class::collision::{
        Collision,
        CollisionBlockClassComponent,
    },
    compute,
    entity::{
        chunk::Chunk,
        snapshot::Snapshot,
    },
    messages::{
        client::ClientAccept,
        ActionsUnpacker,
        StatePacker,
        StateUnpacker,
    },
    pack::Packer,
    script_registry::ScriptRegistry,
    system::{
        actor_class_loading::ActorClassLoadingSystem,
        block_class_loading::BlockClassLoadingSystem,
        list_loading::List,
    },
    ChunkData,
};
use voxbrix_protocol::{
    server::Packet,
    Channel,
};

mod data;
mod player_event;
mod process;

pub enum SharedEvent {
    ChunkLoaded {
        data: ChunkData,
        data_encoded: Arc<Vec<u8>>,
    },
    ChunkGeneration(Chunk),
}

// Server loop input
pub enum ServerEvent {
    Process,
    AddPlayer {
        player: Player,
        client_tx: SharedSender<ClientEvent>,
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

pub struct ServerLoop {
    pub database: Arc<Database>,
    pub event_rx: Receiver<ServerEvent>,
}

impl ServerLoop {
    pub async fn run(self) {
        let Self { database, event_rx } = self;

        let (shared_event_tx, shared_event_rx) = flume::unbounded();

        let actor_class_loading_system = ActorClassLoadingSystem::load_data()
            .await
            .expect("loading actor classes");

        let block_class_loading_system = BlockClassLoadingSystem::load_data()
            .await
            .expect("loading block classes");

        let state_components_label_map = List::load(STATE_COMPONENTS_PATH)
            .await
            .expect("state component list not found")
            .into_label_map();

        let class_ac =
            ClassActorComponent::new(state_components_label_map.get("actor_class").unwrap());
        let position_ac =
            PositionActorComponent::new(state_components_label_map.get("actor_position").unwrap());
        let velocity_ac =
            VelocityActorComponent::new(state_components_label_map.get("actor_velocity").unwrap());
        let orientation_ac = OrientationActorComponent::new(
            state_components_label_map.get("actor_orientation").unwrap(),
        );
        let player_ac = PlayerActorComponent::new();
        let chunk_activation_ac = ChunkActivationActorComponent::new();

        let mut model_acc =
            ModelActorClassComponent::new(state_components_label_map.get("actor_model").unwrap());

        let status_cc = StatusChunkComponent::new();
        let cache_cc = CacheChunkComponent::new();

        let class_bc = ClassBlockComponent::new();
        let mut collision_bcc = CollisionBlockClassComponent::new();

        let position_system = PositionSystem::new();

        let actor_model_label_map = List::load(ACTOR_MODEL_LIST_PATH)
            .await
            .expect("loading actor model label map")
            .into_label_map();

        actor_class_loading_system
            .load_component("model", &mut model_acc, |desc: String| {
                actor_model_label_map.get(&desc).ok_or_else(|| {
                    anyhow::Error::msg(format!("model \"{}\" not found in the model list", desc))
                })
            })
            .expect("unable to load model actor class component");

        let actor_class_label_map = actor_class_loading_system.into_label_map();

        block_class_loading_system
            .load_component("collision", &mut collision_bcc, |desc: Collision| Ok(desc))
            .expect("unable to load collision block class component");

        let block_class_label_map = block_class_loading_system.into_label_map();

        // TODO
        let action_label_map = List::load(ACTION_LIST)
            .await
            .expect("loading actor model label map")
            .into_label_map();

        let mut engine_config = wasmtime::Config::new();

        engine_config
            .wasm_threads(false)
            .wasm_multi_value(false)
            .wasm_multi_memory(false);

        let engine = wasmtime::Engine::new(&engine_config).expect("wasm engine failed to start");

        let mut script_registry =
            ScriptRegistry::load(engine, SERVER_LOOP_SCRIPT_LIST, SERVER_LOOP_SCRIPT_DIR)
                .await
                .expect("failed to load scripts");

        data::setup_script_registry(&mut script_registry);

        let action_script_map = Map::load(ACTION_SCRIPT_MAP)
            .await
            .expect("failed to load action-script map");

        let script_action_component = ScriptActionComponent::new(
            action_script_map.iter(),
            &action_label_map,
            script_registry.script_label_map(),
        )
        .expect("failed to map actions to scripts");

        let dimension_kind_label_map = List::load(DIMENSION_KIND_LIST)
            .await
            .expect("loading dimension kind label map")
            .into_label_map();

        let shared_event_tx_clone = shared_event_tx.clone();
        let chunk_generation_system = ChunkGenerationSystem::new(
            database.clone(),
            block_class_label_map.clone(),
            dimension_kind_label_map,
            move |chunk, block_classes, packer| {
                let data = ChunkData {
                    chunk,
                    block_classes,
                };

                let data_encoded =
                    Arc::new(packer.pack_to_vec(&ClientAccept::ChunkData(data.clone())));

                let _ = shared_event_tx_clone.send(SharedEvent::ChunkLoaded { data, data_encoded });
            },
        )
        .await;

        let mut send_status_interval = time::interval(PROCESS_INTERVAL);
        send_status_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut stream = stream::poll_fn(|cx| {
            send_status_interval
                .poll_tick(cx)
                .map(|_| Some(ServerEvent::Process))
        })
        .or(event_rx)
        .or(shared_event_rx.stream().map(ServerEvent::SharedEvent));

        let storage = StorageThread::new();

        let mut shared_data = SharedData {
            database,
            shared_event_tx,
            packer: Packer::new(),
            actor_registry: ActorRegistry::new(),

            client_pc: ClientPlayerComponent::new(),
            actor_pc: ActorPlayerComponent::new(),
            chunk_update_pc: ChunkUpdatePlayerComponent::new(),
            chunk_view_pc: ChunkViewPlayerComponent::new(),

            class_ac,
            position_ac,
            velocity_ac,
            orientation_ac,
            player_ac,
            chunk_activation_ac,

            model_acc,

            class_bc,

            collision_bcc,

            status_cc,
            cache_cc,

            actor_class_label_map,
            block_class_label_map,
            action_label_map,

            position_system,
            chunk_activation_system: ChunkActivationSystem::new(),
            chunk_generation_system,

            script_registry,

            script_action_component,

            storage,

            snapshot: Snapshot(1),

            state_packer: StatePacker::new(),
            state_unpacker: StateUnpacker::new(),
            actions_unpacker: ActionsUnpacker::new(),

            last_process_time: Instant::now(),

            remove_queue: EntityRemoveQueue::new(),
        };

        while let Some(event) = stream.next().await {
            shared_data.remove_entities();

            match event {
                ServerEvent::Process => {
                    let rt_handle = Handle::current();
                    compute!((shared_data) Process {
                        shared_data: &mut shared_data,
                        rt_handle,
                    }.run());
                },
                ServerEvent::AddPlayer { player, client_tx } => {
                    shared_data.add_player(player, client_tx);
                },
                ServerEvent::PlayerEvent {
                    player,
                    channel,
                    data,
                } => {
                    if channel == BASE_CHANNEL {
                        PlayerEvent {
                            shared_data: &mut shared_data,
                            player,
                            data,
                        }
                        .run();
                    }
                },
                ServerEvent::RemovePlayer { player } => {
                    shared_data.remove_player(&player);
                },
                ServerEvent::SharedEvent(event) => {
                    match event {
                        SharedEvent::ChunkLoaded {
                            data: chunk_data,
                            data_encoded,
                        } => shared_data.chunk_loaded(chunk_data, data_encoded),
                        SharedEvent::ChunkGeneration(chunk) => {
                            shared_data.chunk_generation_system.generate_chunk(chunk);
                        },
                    }
                },
                ServerEvent::ServerConnectionClosed => return,
            }
        }
    }
}
