use crate::{
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
    world::{
        EntityRemoveQueue,
        World,
    },
    Local,
    Shared,
    BASE_CHANNEL,
    PROCESS_INTERVAL,
};
use flume::Receiver as SharedReceiver;
use futures_lite::stream::{
    self,
    StreamExt,
};
use local_channel::mpsc::{
    Receiver,
    Sender,
};
use std::{
    rc::Rc,
    time::Instant,
};
use tokio::time::{
    self,
    MissedTickBehavior,
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
    entity::{
        actor_model::ActorModel,
        chunk::Chunk,
        snapshot::Snapshot,
        state_component::StateComponent,
    },
    messages::{
        client::ClientAccept,
        StatePacker,
    },
    pack::Packer,
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

pub enum SharedEvent {
    ChunkLoaded {
        data: ChunkData,
        data_encoded: SendRc<Vec<u8>>,
    },
    ChunkGeneration(Chunk),
}

// Server loop input
pub enum ServerEvent {
    Process,
    AddPlayer {
        player: Player,
        client_tx: Sender<ClientEvent>,
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

/// Packs data into Rc in one thread and extract it in another
pub struct SendRc<T>(Rc<T>);

impl<T> SendRc<T>
where
    T: Send,
{
    pub fn new(data: T) -> Self {
        Self(Rc::new(data))
    }

    pub fn extract(self) -> Rc<T> {
        self.0
    }
}

// Safe, as the Rc counter in the container can not be incremented (clone)
// and can be decremented (drop) only once, with dropping the container.
unsafe impl<T: Send> Send for SendRc<T> {}

// Safe, references to the container can safely be passed between threads
// because one can only get access to the underlying Rc by consuming.
// the container, which does not have Clone
unsafe impl<T: Sync> Sync for SendRc<T> {}

pub async fn run(
    local: &'static Local,
    shared: &'static Shared,
    event_rx: Receiver<ServerEvent>,
    event_shared_rx: SharedReceiver<SharedEvent>,
) {
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

    let class_ac = ClassActorComponent::new(state_components_label_map.get("actor_class").unwrap());
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

    let chunk_generation_system = ChunkGenerationSystem::new(
        shared,
        block_class_label_map.clone(),
        |chunk, block_classes, packer| {
            let data = ChunkData {
                chunk,
                block_classes,
            };

            let data_encoded =
                SendRc::new(packer.pack_to_vec(&ClientAccept::ChunkData(data.clone())));

            let _ = shared
                .event_tx
                .send(SharedEvent::ChunkLoaded { data, data_encoded });
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
    .or(event_shared_rx.stream().map(ServerEvent::SharedEvent));

    let storage = StorageThread::new();

    let mut world = World {
        local,
        shared,

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

        position_system,
        chunk_activation_system: ChunkActivationSystem::new(),
        chunk_generation_system,

        storage,

        snapshot: Snapshot(1),

        server_state: StatePacker::new(),

        last_process_time: Instant::now(),

        remove_queue: EntityRemoveQueue::new(),
    };

    while let Some(event) = stream.next().await {
        world.remove_entities();

        match event {
            ServerEvent::Process => {
                world = world.process();
            },
            ServerEvent::AddPlayer { player, client_tx } => {
                world.add_player(player, client_tx);
            },
            ServerEvent::PlayerEvent {
                player,
                channel,
                data,
            } => {
                if channel == BASE_CHANNEL {
                    world.player_event(player, data);
                }
            },
            ServerEvent::RemovePlayer { player } => {
                world.remove_player(&player);
            },
            ServerEvent::SharedEvent(event) => {
                match event {
                    SharedEvent::ChunkLoaded {
                        data: chunk_data,
                        data_encoded,
                    } => world.chunk_loaded(chunk_data, data_encoded),
                    SharedEvent::ChunkGeneration(chunk) => {
                        world.chunk_generation_system.generate_chunk(chunk);
                    },
                }
            },
            ServerEvent::ServerConnectionClosed => return,
        }
    }
}
