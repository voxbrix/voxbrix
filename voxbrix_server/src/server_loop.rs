use crate::{
    assets::{
        ACTION_HANDLER_MAP,
        DIMENSION_KIND_LIST,
        EFFECTS_DIR,
        SERVER_LOOP_SCRIPT_DIR,
        SERVER_LOOP_SCRIPT_LIST,
    },
    component::{
        action::handler::{
            HandlerActionComponent,
            HandlerSetDescriptor,
        },
        actor::{
            chunk_activation::ChunkActivationActorComponent,
            class::ClassActorComponent,
            effect::EffectActorComponent,
            orientation::OrientationActorComponent,
            player::PlayerActorComponent,
            position::{
                PositionActorComponent,
                PositionChanges,
            },
            projectile::ProjectileActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::model::ModelActorClassComponent,
        block::class::ClassBlockComponent,
        chunk::{
            cache::CacheChunkComponent,
            status::StatusChunkComponent,
        },
        effect::snapshot_handler::SnapshotHandlerEffectComponent,
        player::{
            actor::ActorPlayerComponent,
            chunk_update::ChunkUpdatePlayerComponent,
            chunk_view::ChunkViewPlayerComponent,
            client::{
                ClientEvent,
                ClientPlayerComponent,
            },
            dispatches_packer::DispatchesPackerPlayerComponent,
        },
    },
    entity::{
        actor::ActorRegistry,
        player::Player,
    },
    resource::{
        script_shared_data,
        shared_event::SharedEvent,
    },
    storage::StorageThread,
    system::{
        chunk_activation::ChunkActivationSystem,
        chunk_add::ChunkAddSystem,
        chunk_generation::ChunkGenerationSystem,
        entity_removal::{
            EntityRemovalCheckSystem,
            EntityRemovalSystem,
        },
        map_loading::Map,
        player_add::{
            PlayerAddData,
            PlayerAddSystem,
        },
        position::PositionSystem,
    },
    PROCESS_INTERVAL,
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
use std::sync::Arc;
use tokio::{
    runtime::Handle,
    time::{
        self,
        MissedTickBehavior,
    },
};
use voxbrix_common::{
    assets::{
        ACTION_LIST_PATH,
        ACTOR_CLASS_DIR,
        ACTOR_CLASS_LIST_PATH,
        ACTOR_MODEL_LIST_PATH,
        BLOCK_CLASS_DIR,
        BLOCK_CLASS_LIST_PATH,
        EFFECT_LIST_PATH,
        UPDATE_LIST_PATH,
    },
    component::block_class::collision::{
        Collision,
        CollisionBlockClassComponent,
    },
    compute,
    entity::{
        action::Action,
        actor::Actor,
        actor_class::ActorClass,
        actor_model::ActorModel,
        block_class::BlockClass,
        chunk::DimensionKind,
        effect::Effect,
        snapshot::ServerSnapshot,
        update::Update,
    },
    messages::{
        client::ClientAccept,
        ClientActionsUnpacker,
        UpdatesPacker,
        UpdatesUnpacker,
    },
    pack::Packer,
    resource::{
        process_timer::ProcessTimer,
        removal_queue::RemovalQueue,
    },
    script_registry::ScriptRegistryBuilder,
    system::component_map::ComponentMap,
    ChunkData,
    LabelLibrary,
};
use voxbrix_protocol::server::ReceivedData;
use voxbrix_world::World;

mod player_event;
mod process;

// Server loop input
pub enum ServerEvent {
    Process,
    AddPlayer {
        player: Player,
        client_tx: SharedSender<ClientEvent>,
        session_id: u64,
    },
    PlayerEvent {
        player: Player,
        message: ReceivedData,
        session_id: u64,
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

        let mut world = World::new();

        let (shared_event_tx, shared_event_rx) = flume::unbounded();

        let mut label_library = LabelLibrary::new();

        label_library
            .load::<Update>(UPDATE_LIST_PATH)
            .await
            .expect("Update list loading error");

        label_library
            .load::<ActorClass>(ACTOR_CLASS_LIST_PATH)
            .await
            .expect("ActorClass list loading error");

        label_library
            .load::<BlockClass>(BLOCK_CLASS_LIST_PATH)
            .await
            .expect("BlockClass list loading error");

        label_library
            .load::<ActorModel>(ACTOR_MODEL_LIST_PATH)
            .await
            .expect("ActorModel list loading error");

        label_library
            .load::<Effect>(EFFECT_LIST_PATH)
            .await
            .expect("Effect list loading error");

        label_library
            .load::<Action>(ACTION_LIST_PATH)
            .await
            .expect("Action list loading error");

        label_library
            .load::<DimensionKind>(DIMENSION_KIND_LIST)
            .await
            .expect("DimensionKind list loading error");

        let actor_class_component_map =
            ComponentMap::<ActorClass>::load_data(ACTOR_CLASS_DIR, &label_library)
                .await
                .expect("loading actor classes");

        let block_class_component_map =
            ComponentMap::<BlockClass>::load_data(BLOCK_CLASS_DIR, &label_library)
                .await
                .expect("loading block classes");

        let effect_component_map = ComponentMap::load_data(EFFECTS_DIR, &label_library)
            .await
            .expect("unable to load effect component map");

        let class_ac = ClassActorComponent::new(label_library.get("actor_class").unwrap());
        let position_ac = PositionActorComponent::new(label_library.get("actor_position").unwrap());
        let velocity_ac = VelocityActorComponent::new(label_library.get("actor_velocity").unwrap());
        let orientation_ac =
            OrientationActorComponent::new(label_library.get("actor_orientation").unwrap());
        let player_ac = PlayerActorComponent::new();
        let chunk_activation_ac = ChunkActivationActorComponent::new();

        let snapshot_handler_ec =
            SnapshotHandlerEffectComponent::new(&effect_component_map, &label_library)
                .expect("unable to load snapshot handler effect component");

        let model_acc = ModelActorClassComponent::new(
            &actor_class_component_map,
            &label_library,
            label_library.get("actor_model").unwrap(),
            "model",
            |desc: String| {
                label_library
                    .get(&desc)
                    .ok_or_else(|| anyhow::anyhow!("model \"{}\" not found", desc))
            },
        )
        .expect("unable to load CollisionBlockClassComponent");

        let status_cc = StatusChunkComponent::new();
        let cache_cc = CacheChunkComponent::new();

        let class_bc = ClassBlockComponent::new();
        let collision_bcc = CollisionBlockClassComponent::new(
            &block_class_component_map,
            &label_library,
            "collision",
            |v: Collision| Ok(v),
        )
        .expect("unable to load CollisionBlockClassComponent");

        let mut engine_config = wasmtime::Config::new();

        engine_config
            .wasm_multi_value(false)
            .wasm_multi_memory(false);

        let engine = wasmtime::Engine::new(&engine_config).expect("wasm engine failed to start");

        let script_registry = script_shared_data::setup_script_registry(
            ScriptRegistryBuilder::load(engine, SERVER_LOOP_SCRIPT_LIST, SERVER_LOOP_SCRIPT_DIR)
                .await
                .expect("failed to load scripts"),
        );

        label_library.add_label_map(script_registry.script_label_map().clone());

        let action_handler_map = Map::<HandlerSetDescriptor>::load(ACTION_HANDLER_MAP)
            .await
            .expect("failed to load action-script map");

        let handler_action_component =
            HandlerActionComponent::load_from_descriptor(&label_library, &|label| {
                action_handler_map.get(label)
            })
            .expect("failed to map actions to scripts");

        let shared_event_tx_clone = shared_event_tx.clone();

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

        world.add(database);
        world.add(shared_event_tx);
        world.add(Packer::new());
        world.add(ActorRegistry::new());

        world.add(ClientPlayerComponent::new());
        world.add(DispatchesPackerPlayerComponent::new());
        world.add(ActorPlayerComponent::new());
        world.add(ChunkUpdatePlayerComponent::new());
        world.add(ChunkViewPlayerComponent::new());

        world.add(class_ac);
        world.add(position_ac);
        world.add(PositionChanges::new());
        world.add(velocity_ac);
        world.add(orientation_ac);
        world.add(player_ac);
        world.add(chunk_activation_ac);
        world.add(EffectActorComponent::new(
            label_library.get("actor_effect").unwrap(),
        ));
        world.add(ProjectileActorComponent::new());

        world.add(model_acc);

        world.add(class_bc);

        world.add(collision_bcc);

        world.add(snapshot_handler_ec);

        world.add(status_cc);
        world.add(cache_cc);

        world.add(label_library);

        let chunk_generation_system = world
            .get_data::<ChunkGenerationSystem>()
            .spawn(move |chunk, block_classes, packer| {
                let data = ChunkData {
                    chunk,
                    block_classes,
                };

                let data_encoded = packer
                    .pack_to_vec(&ClientAccept::ChunkData(data.clone()))
                    .into();

                let _ = shared_event_tx_clone.send(SharedEvent::ChunkLoaded { data, data_encoded });
            })
            .await;

        world.add(chunk_generation_system);

        world.add(PositionSystem);
        world.add(ChunkActivationSystem::new());

        world.add(script_registry);

        world.add(handler_action_component);

        world.add(storage);

        world.add(ServerSnapshot(1));

        world.add(UpdatesPacker::new());
        world.add(UpdatesUnpacker::new());
        world.add(ClientActionsUnpacker::new());

        world.add(ProcessTimer::new());

        world.add(RemovalQueue::<Actor>::new());
        world.add(RemovalQueue::<Player>::new());
        world.add(Handle::current());

        while let Some(event) = stream.next().await {
            if world.get_data::<EntityRemovalCheckSystem>().run() {
                world.get_data::<EntityRemovalSystem>().run();
            }

            match event {
                ServerEvent::Process => {
                    compute!((world) Process {
                        world: &mut world,
                    }.run());
                },
                ServerEvent::AddPlayer {
                    player,
                    client_tx,
                    session_id,
                } => {
                    // TODO remove player
                    world.get_data::<PlayerAddSystem>().run(PlayerAddData {
                        player,
                        tx: client_tx,
                        session_id,
                    });
                },
                ServerEvent::PlayerEvent {
                    player,
                    message,
                    session_id,
                } => {
                    // Filter out outdated messages
                    // and other channels
                    if world
                        .get_resource_ref::<ClientPlayerComponent>()
                        .get(&player)
                        .map(|c| c.session_id == session_id)
                        .unwrap_or(false)
                    {
                        PlayerEvent {
                            world: &mut world,
                            player,
                            message,
                        }
                        .run();
                    }
                },
                ServerEvent::SharedEvent(event) => {
                    match event {
                        SharedEvent::ChunkLoaded {
                            data: chunk_data,
                            data_encoded,
                        } => {
                            world
                                .get_data::<ChunkAddSystem>()
                                .run(chunk_data, data_encoded);
                        },
                    }
                },
                ServerEvent::ServerConnectionClosed => return,
            }
        }
    }
}
