use crate::{
    assets::{
        SERVER_LOOP_SCRIPT_DIR,
        SERVER_LOOP_SCRIPT_LIST,
    },
    component::{
        action::handler::HandlerActionComponent,
        actor::{
            chunk_activation::ChunkActivationActorComponent,
            class::ClassActorComponent,
            effect::EffectActorComponent,
            movement_change::MovementChangeActorComponent,
            orientation::OrientationActorComponent,
            player::PlayerActorComponent,
            position::PositionActorComponent,
            projectile::ProjectileActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::{
            block_collision::BlockCollisionActorClassComponent,
            health::HealthActorClassComponent,
            hitbox::HitboxActorClassComponent,
            model::ModelActorClassComponent,
        },
        block::{
            class::ClassBlockComponent,
            environment::EnvironmentBlockComponent,
            metadata::MetadataBlockComponent,
        },
        chunk::{
            cache::CacheChunkComponent,
            status::StatusChunkComponent,
        },
        dimension_kind::{
            boundary::BoundaryDimensionKindComponent,
            player_chunk_view::PlayerChunkViewDimensionKindComponent,
        },
        effect::snapshot_handler::SnapshotHandlerEffectComponent,
        player::{
            actor::ActorPlayerComponent,
            chunk_update::ChunkUpdatePlayerComponent,
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
        projectile_actor_collisions::ProjectileActorCollisions,
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
        player_add::{
            PlayerAddData,
            PlayerAddSystem,
        },
        position::PositionSystem,
    },
    PROCESS_INTERVAL,
};
use anyhow::Context as _;
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
    any,
    sync::Arc,
};
use tokio::{
    runtime::Handle,
    time::{
        self,
        MissedTickBehavior,
    },
};
use voxbrix_common::{
    component::block_class::collision::CollisionBlockClassComponent,
    compute,
    entity::{
        action::Action,
        actor::Actor,
        actor_class::ActorClass,
        actor_model::ActorModel,
        block_class::BlockClass,
        block_environment::BlockEnvironment,
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
        component_map::ComponentMap,
        process_timer::ProcessTimer,
        removal_queue::RemovalQueue,
    },
    script_registry::ScriptRegistryBuilder,
    ChunkData,
    LabelLibrary,
    StaticEntity,
};
use voxbrix_protocol::server::ReceivedData;
use voxbrix_world::{
    Initialization,
    World,
};

mod player_event;
mod process;

async fn label_load<T>(label_library: &mut LabelLibrary) -> Result<(), anyhow::Error>
where
    T: StaticEntity,
{
    label_library
        .load::<T>()
        .await
        .with_context(|| format!("\"{}\" list loading error", any::type_name::<T>()))
}

async fn init_add<T>(world: &mut World) -> Result<(), anyhow::Error>
where
    T: Initialization<Error = anyhow::Error>,
{
    world
        .initialize_add::<T>()
        .await
        .with_context(|| format!("\"{}\" initialization error", any::type_name::<T>()))
}

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
    pub async fn run(self) -> Result<(), anyhow::Error> {
        let Self { database, event_rx } = self;

        let mut world = World::new();

        let (shared_event_tx, shared_event_rx) = flume::unbounded();

        let mut label_library = LabelLibrary::new();

        label_load::<Update>(&mut label_library).await?;
        label_load::<ActorClass>(&mut label_library).await?;
        label_load::<BlockClass>(&mut label_library).await?;
        label_load::<BlockEnvironment>(&mut label_library).await?;
        label_load::<ActorModel>(&mut label_library).await?;
        label_load::<Effect>(&mut label_library).await?;
        label_load::<Action>(&mut label_library).await?;
        label_load::<DimensionKind>(&mut label_library).await?;

        world.add(label_library);

        init_add::<ComponentMap<ActorClass>>(&mut world).await?;
        init_add::<ComponentMap<BlockClass>>(&mut world).await?;
        init_add::<ComponentMap<BlockEnvironment>>(&mut world).await?;
        init_add::<ComponentMap<Effect>>(&mut world).await?;

        init_add::<ClassActorComponent>(&mut world).await?;
        init_add::<PositionActorComponent>(&mut world).await?;
        init_add::<VelocityActorComponent>(&mut world).await?;
        init_add::<OrientationActorComponent>(&mut world).await?;
        init_add::<PlayerActorComponent>(&mut world).await?;
        init_add::<ChunkActivationActorComponent>(&mut world).await?;
        init_add::<EffectActorComponent>(&mut world).await?;

        init_add::<SnapshotHandlerEffectComponent>(&mut world).await?;

        init_add::<ModelActorClassComponent>(&mut world).await?;
        init_add::<HealthActorClassComponent>(&mut world).await?;
        init_add::<HitboxActorClassComponent>(&mut world).await?;
        init_add::<BlockCollisionActorClassComponent>(&mut world).await?;

        init_add::<StatusChunkComponent>(&mut world).await?;
        init_add::<CacheChunkComponent>(&mut world).await?;

        init_add::<ClassBlockComponent>(&mut world).await?;
        init_add::<EnvironmentBlockComponent>(&mut world).await?;
        init_add::<MetadataBlockComponent>(&mut world).await?;

        init_add::<CollisionBlockClassComponent>(&mut world).await?;

        init_add::<ComponentMap<DimensionKind>>(&mut world).await?;
        init_add::<BoundaryDimensionKindComponent>(&mut world).await?;
        init_add::<PlayerChunkViewDimensionKindComponent>(&mut world).await?;

        let mut engine_config = wasmtime::Config::new();

        engine_config
            .wasm_multi_value(false)
            .wasm_multi_memory(false);

        let engine =
            wasmtime::Engine::new(&engine_config).context("wasm engine failed to start")?;

        let script_registry = script_shared_data::setup_script_registry(
            ScriptRegistryBuilder::load(engine, SERVER_LOOP_SCRIPT_LIST, SERVER_LOOP_SCRIPT_DIR)
                .await
                .context("failed to load scripts")?,
        );

        world
            .get_resource_mut::<LabelLibrary>()
            .add_label_map(script_registry.script_label_map().clone());

        init_add::<HandlerActionComponent>(&mut world).await?;

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

        init_add::<MovementChangeActorComponent>(&mut world).await?;
        init_add::<ProjectileActorComponent>(&mut world).await?;

        world.add(ProjectileActorCollisions::new());

        let chunk_generation_system = world
            .get_data::<ChunkGenerationSystem>()
            .spawn(
                move |chunk, block_classes, block_environment, block_metadata, packer| {
                    let data = ChunkData {
                        chunk,
                        block_classes,
                        block_environment,
                        block_metadata,
                    };

                    let data_encoded = packer
                        .pack_to_vec(&ClientAccept::ChunkData(data.clone()))
                        .into();

                    let _ =
                        shared_event_tx_clone.send(SharedEvent::ChunkLoaded { data, data_encoded });
                },
            )
            .await;

        world.add(chunk_generation_system);

        world.add(PositionSystem);
        world.add(ChunkActivationSystem::new());

        world.add(script_registry);

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
                    world
                        .get_resource_mut::<RemovalQueue<Player>>()
                        .enqueue(player);
                    world.get_data::<EntityRemovalSystem>().run();
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
                ServerEvent::ServerConnectionClosed => return Ok(()),
            }
        }

        Ok(())
    }
}
