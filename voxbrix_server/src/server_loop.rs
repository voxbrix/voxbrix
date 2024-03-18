use crate::{
    assets::{
        ACTION_LIST_PATH,
        SCRIPTS_DIR,
        SCRIPT_LIST_PATH,
    },
    component::{
        actor::{
            chunk_activation::ChunkActivationActorComponent,
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            player::PlayerActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::model::ModelActorClassComponent,
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
    component::{
        block::class::ClassBlockComponent,
        block_class::collision::{
            Collision,
            CollisionBlockClassComponent,
        },
    },
    compute,
    entity::{
        action::Action,
        actor_model::ActorModel,
        chunk::Chunk,
        snapshot::Snapshot,
        state_component::StateComponent,
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
            .into_label_map(StateComponent::from_usize);

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
            .into_label_map(ActorModel::from_usize);

        actor_class_loading_system
            .load_component("model", &mut model_acc, |desc: String| {
                actor_model_label_map.get(&desc).ok_or_else(|| {
                    anyhow::Error::msg(format!("model \"{}\" not found in the model list", desc))
                })
            })
            .expect("unable to load collision block class component");

        let actor_class_label_map = actor_class_loading_system.into_label_map();

        block_class_loading_system
            .load_component("collision", &mut collision_bcc, |desc: Collision| Ok(desc))
            .expect("unable to load collision block class component");

        let block_class_label_map = block_class_loading_system.into_label_map();

        // TODO
        let action_label_map = List::load(ACTION_LIST_PATH)
            .await
            .expect("loading actor model label map")
            .into_label_map(|i| Action(i as u64));

        let mut engine_config = wasmtime::Config::new();

        engine_config
            .wasm_threads(false)
            .wasm_reference_types(false)
            .wasm_multi_value(false)
            .wasm_multi_memory(false);

        let engine = wasmtime::Engine::new(&engine_config).expect("wasm engine failed to start");

        let mut script_registry = ScriptRegistry::load(engine, SCRIPT_LIST_PATH, SCRIPTS_DIR)
            .await
            .expect("failed to load scripts");

        unsafe {
            script_registry.func_wrap(
                "env",
                "handle_panic",
                |mut caller: wasmtime::Caller<'_, _>, msg_ptr: u32, msg_len: u32| {
                    let ptr = msg_ptr as usize;
                    let len = msg_len as usize;
                    let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                    let msg = std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();

                    panic!("script ended with panic: {}", msg);
                },
            );
        }

        unsafe {
            script_registry.func_wrap(
                "env",
                "log_message",
                |mut caller: wasmtime::Caller<
                    '_,
                    voxbrix_common::script_registry::ScriptData<data::ScriptSharedData<'_>>,
                >,
                 msg_ptr: u32,
                 msg_len: u32| {
                    let ptr = msg_ptr as usize;
                    let len = msg_len as usize;
                    let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                    let msg = std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();

                    log::error!("{}", msg);
                },
            );
        }

        // script_registry
        // .linker()
        // .func_wrap(
        // "env",
        // "get_block_class_by_label",
        // move |mut caller: wasmtime::Caller<'_, Option<data::ScriptSharedData<'_>>>, ptr: u32, len: u32| -> u64 {
        // let ptr = ptr as usize;
        // let len = len as usize;
        // let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        // let label =
        // std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();
        //
        // let data = caller.data().as_ref()
        // .expect("store data is not injected");
        //
        // data.block_class_label_map.get(label).unwrap().0
        // },
        // )
        // .unwrap();
        //
        // script_registry
        // .linker()
        // .func_wrap(
        // "env",
        // "get_class_of_block",
        // |mut caller: wasmtime::Caller<'_, Option<data::ScriptSharedData<'_>>>, block_class: u64| {
        // let data = caller.data().as_ref()
        // .expect("store data is not injected");
        //
        // data.class_bc.get_chunk()
        //
        // data.class_bc.get_mut_chunk
        // },
        // )
        // .unwrap();
        //
        // script_registry
        // .linker()
        // .func_wrap(
        // "env",
        // "",
        // |mut caller: wasmtime::Caller<'_, Option<data::ScriptSharedData<'_>>>, block_class: u64| {
        // let data = caller.data_mut().as_mut()
        // .expect("store data is not injected");
        //
        // data.class_bc.get_mut_chunk
        // },
        // )
        // .unwrap();

        let shared_event_tx_clone = shared_event_tx.clone();
        let chunk_generation_system = ChunkGenerationSystem::new(
            database.clone(),
            block_class_label_map.clone(),
            move |chunk, block_classes, packer| {
                let data = ChunkData {
                    chunk,
                    block_classes,
                };

                let data_encoded =
                    Arc::new(packer.pack_to_vec(&ClientAccept::ChunkData(data.clone())));

                let _ = shared_event_tx_clone.send(SharedEvent::ChunkLoaded { data, data_encoded });
            },
        );

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
