use crate::{
    component::{
        action::script::ScriptActionComponent,
        actor::{
            chunk_activation::{
                ActorChunkActivation,
                ChunkActivationActorComponent,
            },
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
            status::{
                ChunkStatus,
                StatusChunkComponent,
            },
        },
        player::{
            actions_packer::ActionsPackerPlayerComponent,
            actor::ActorPlayerComponent,
            chunk_update::ChunkUpdatePlayerComponent,
            chunk_view::{
                ChunkView,
                ChunkViewPlayerComponent,
            },
            client::{
                Client,
                ClientEvent,
                ClientPlayerComponent,
                SendData,
            },
        },
    },
    entity::{
        actor::ActorRegistry,
        player::Player,
    },
    server_loop::SharedEvent,
    storage::StorageThread,
    system::{
        chunk_activation::ChunkActivationSystem,
        chunk_generation::ChunkGenerationSystem,
        position::PositionSystem,
    },
    BASE_CHANNEL,
    PLAYER_CHUNK_VIEW_RADIUS,
};
use flume::Sender;
use log::debug;
use nohash_hasher::IntSet;
use redb::Database;
use server_loop_api::{
    ActionInput,
    GetTargetBlockRequest,
    GetTargetBlockResponse,
    SetClassOfBlockRequest,
};
use std::{
    mem,
    sync::Arc,
    time::Instant,
};
use voxbrix_common::{
    component::{
        actor::position::Position,
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::{
        action::Action,
        actor::Actor,
        actor_class::ActorClass,
        block::BLOCKS_IN_CHUNK_EDGE,
        block_class::BlockClass,
        chunk::Chunk,
        snapshot::Snapshot,
    },
    messages::{
        ActionsPacker,
        ActionsUnpacker,
        StatePacker,
        StateUnpacker,
    },
    pack::{
        self,
        Packer,
    },
    script_registry::{
        self,
        ScriptData,
        ScriptRegistry,
        ScriptRegistryBuilder,
    },
    system::position,
    ChunkData,
    LabelMap,
};
use wasmtime::Caller;

pub struct EntityRemoveQueue(Option<EntityRemoveQueueInner>);

struct EntityRemoveQueueInner {
    is_not_empty: bool,
    actors: IntSet<Actor>,
    players: IntSet<Player>,
}

impl EntityRemoveQueueInner {
    fn new() -> Option<Self> {
        Some(Self {
            is_not_empty: false,
            actors: IntSet::default(),
            players: IntSet::default(),
        })
    }

    fn remove_player(&mut self, player: &Player) {
        self.players.insert(*player);
        self.is_not_empty = true;
    }
}

impl EntityRemoveQueue {
    pub fn new() -> Self {
        Self(EntityRemoveQueueInner::new())
    }

    pub fn remove_player(&mut self, player: &Player) {
        self.0
            .as_mut()
            .expect("EntityRemoveQueue is taken")
            .remove_player(player)
    }

    fn take(&mut self) -> EntityRemoveQueueInner {
        self.0.take().expect("EntityRemoveQueue is taken")
    }

    fn return_taken(&mut self, taken: EntityRemoveQueueInner) {
        self.0 = Some(taken);
    }
}

pub struct SendMutPtr<T>(*mut T);
unsafe impl<T> Send for SendMutPtr<T> where T: Send {}

impl<T> SendMutPtr<T> {
    pub fn new(value: &mut T) -> Self {
        Self(value)
    }

    pub unsafe fn get<'a>(&'a self) -> &'a T {
        &*self.0
    }

    pub unsafe fn get_mut<'a>(&'a mut self) -> &'a mut T {
        &mut *self.0
    }
}

pub struct SendPtr<T>(*const T);
unsafe impl<T> Send for SendPtr<T> where T: Sync {}

impl<T> SendPtr<T> {
    pub fn new(value: &T) -> Self {
        Self(value)
    }

    pub unsafe fn get<'a>(&'a self) -> &'a T {
        unsafe { &*self.0 }
    }
}

/// For safely retrieving the data:
/// 1. Make sure that the pointers (SendPtr and SendMutPtr) do not live after the wrapped function
///    returns. Should be easy: do NOT replace SendPtr- and SendMutPtr-typed fields of the
///    ScriptSharedData in the wrapped functions.
/// 2. Pointers created must not violate borrowing rules.
pub struct ScriptSharedData {
    pub snapshot: Snapshot,
    pub actor_pc: SendPtr<ActorPlayerComponent>,
    pub actions_packer_pc: SendMutPtr<ActionsPackerPlayerComponent>,
    pub chunk_view_pc: SendPtr<ChunkViewPlayerComponent>,
    pub position_ac: SendPtr<PositionActorComponent>,
    pub block_class_label_map: SendPtr<LabelMap<BlockClass>>,
    pub class_bc: SendMutPtr<ClassBlockComponent>,
    pub collision_bcc: SendPtr<CollisionBlockClassComponent>,
}

// Try to make unsafe blocks only output owned types.
pub fn setup_script_registry(
    mut registry: ScriptRegistryBuilder<ScriptSharedData>,
) -> ScriptRegistry<ScriptSharedData> {
    fn handle_panic(caller: Caller<ScriptData<ScriptSharedData>>, msg_ptr: u32, msg_len: u32) {
        let ptr = msg_ptr as usize;
        let len = msg_len as usize;
        let memory = caller.data().memory();
        let msg = std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();

        panic!("script ended with panic: {}", msg);
    }

    registry.func_wrap("env", "handle_panic", handle_panic);

    fn log_message(caller: Caller<ScriptData<ScriptSharedData>>, msg_ptr: u32, msg_len: u32) {
        let ptr = msg_ptr as usize;
        let len = msg_len as usize;
        let memory = caller.data().memory();
        let msg = std::str::from_utf8(&memory.data(&caller)[ptr .. ptr + len]).unwrap();

        log::error!("{}", msg);
    }

    registry.func_wrap("env", "log_message", log_message);

    fn get_blocks_in_chunk_edge(_: Caller<ScriptData<ScriptSharedData>>) -> u32 {
        BLOCKS_IN_CHUNK_EDGE as u32
    }

    registry.func_wrap("env", "get_blocks_in_chunk_edge", get_blocks_in_chunk_edge);

    fn get_target_block(
        mut caller: Caller<ScriptData<ScriptSharedData>>,
        buf_ptr: u32,
        buf_len: u32,
    ) {
        let ptr = buf_ptr as usize;
        let len = buf_len as usize;
        let memory = caller.data().memory();
        let bytes = &memory.data(&caller)[ptr .. ptr + len];

        let (command, _) =
            pack::decode_from_slice::<GetTargetBlockRequest>(bytes).expect("invalid argument");

        let sd = caller.data().shared();
        let class_bc = unsafe { sd.class_bc.get() };
        let collision_bcc = unsafe { sd.collision_bcc.get() };

        let response = position::get_target_block(
            &Position {
                chunk: command.chunk.into(),
                offset: command.offset.into(),
            },
            command.direction.into(),
            |chunk, block| {
                class_bc
                    .get_chunk(&chunk)
                    .map(|blocks| {
                        let class = blocks.get(block);
                        collision_bcc.get(class).is_some()
                    })
                    .unwrap_or(false)
            },
        )
        .map(|(chunk, block, side)| {
            GetTargetBlockResponse {
                chunk: chunk.into(),
                block: block.into(),
                side: side as u8,
            }
        });

        script_registry::write_script_buffer(&mut caller, response);
    }

    registry.func_wrap("env", "get_target_block", get_target_block);

    fn set_class_of_block(
        mut caller: Caller<ScriptData<ScriptSharedData>>,
        buf_ptr: u32,
        buf_len: u32,
    ) {
        let ptr = buf_ptr as usize;
        let len = buf_len as usize;
        let memory = caller.data().memory();
        let bytes = &memory.data(&caller)[ptr .. ptr + len];

        let (command, _) =
            pack::decode_from_slice::<SetClassOfBlockRequest>(bytes).expect("invalid argument");

        let sd = caller.data_mut().shared_mut();
        let class_bc = unsafe { sd.class_bc.get_mut() };

        let Some(mut classes) = class_bc.get_mut_chunk(&command.chunk.into()) else {
            debug!("changing non-existant chunk");
            return;
        };

        classes.set(command.block.into(), command.block_class.into());
    }

    registry.func_wrap("env", "set_class_of_block", set_class_of_block);

    fn get_block_class_by_label(
        mut caller: Caller<ScriptData<ScriptSharedData>>,
        buf_ptr: u32,
        buf_len: u32,
    ) {
        let ptr = buf_ptr as usize;
        let len = buf_len as usize;
        let memory = caller.data().memory();
        let bytes = &memory.data(&caller)[ptr .. ptr + len];

        let (label, _) = pack::decode_from_slice::<&str>(bytes).expect("invalid argument");

        let sd = caller.data().shared();
        let block_class_label_map = unsafe { sd.block_class_label_map.get() };

        let response = block_class_label_map.get(label);

        script_registry::write_script_buffer(&mut caller, response);
    }

    registry.func_wrap("env", "get_block_class_by_label", get_block_class_by_label);

    fn broadcast_action_local(
        mut caller: Caller<ScriptData<ScriptSharedData>>,
        buf_ptr: u32,
        buf_len: u32,
    ) {
        let ptr = buf_ptr as usize;
        let len = buf_len as usize;
        let memory = caller.data().memory();
        let mut bytes = mem::take(caller.data_mut().buffer());
        bytes.extend_from_slice(&memory.data(&caller)[ptr .. ptr + len]);

        let sd = caller.data_mut().shared_mut();
        let actor_pc = unsafe { sd.actor_pc.get() };
        let actions_packer_pc = unsafe { sd.actions_packer_pc.get_mut() };
        let position_ac = unsafe { sd.position_ac.get() };
        let chunk_view_pc = unsafe { sd.chunk_view_pc.get() };

        // TODO Instead of option, in the future we should have either actor or "acting position"
        // directly as an enum.
        let input: ActionInput = pack::decode_from_slice(&bytes)
            .expect("unable to decode action data")
            .0;

        let action_actor: Option<Actor> = input.actor.map(Into::into);

        let acting_position = position_ac
            .get(&action_actor.expect("actor was not passed by the script"))
            .expect("acting actor has no position");

        for player in chunk_view_pc.iter().filter_map(|(player, chunk_view)| {
            let position = position_ac.get(actor_pc.get(&player)?)?;

            position
                .chunk
                .radius(chunk_view.radius)
                .is_within(&acting_position.chunk)
                .then_some(())?;

            Some(player)
        }) {
            actions_packer_pc
                .get_mut(&player)
                .expect("no action packer found for a player")
                .add_action(input.action.into(), sd.snapshot, (action_actor, input.data));
        }

        *caller.data_mut().buffer() = bytes;
    }

    registry.func_wrap("env", "broadcast_action_local", broadcast_action_local);

    registry.build()
}

/// All components and systems the loop has.
pub struct SharedData {
    pub database: Arc<Database>,
    pub shared_event_tx: Sender<SharedEvent>,
    pub packer: Packer,
    pub actor_registry: ActorRegistry,

    pub client_pc: ClientPlayerComponent,
    pub actor_pc: ActorPlayerComponent,
    pub chunk_update_pc: ChunkUpdatePlayerComponent,
    pub chunk_view_pc: ChunkViewPlayerComponent,
    pub actions_packer_pc: ActionsPackerPlayerComponent,

    pub class_ac: ClassActorComponent,
    pub position_ac: PositionActorComponent,
    pub velocity_ac: VelocityActorComponent,
    pub orientation_ac: OrientationActorComponent,
    pub player_ac: PlayerActorComponent,
    pub chunk_activation_ac: ChunkActivationActorComponent,

    pub model_acc: ModelActorClassComponent,

    pub class_bc: ClassBlockComponent,
    pub collision_bcc: CollisionBlockClassComponent,

    pub status_cc: StatusChunkComponent,
    pub cache_cc: CacheChunkComponent,

    pub actor_class_label_map: LabelMap<ActorClass>,
    pub block_class_label_map: LabelMap<BlockClass>,
    pub action_label_map: LabelMap<Action>,

    pub position_system: PositionSystem,
    pub chunk_activation_system: ChunkActivationSystem,
    pub chunk_generation_system: ChunkGenerationSystem,

    pub script_registry: ScriptRegistry<ScriptSharedData>,

    pub script_action_component: ScriptActionComponent,

    pub storage: StorageThread,

    pub snapshot: Snapshot,

    pub state_packer: StatePacker,
    pub state_unpacker: StateUnpacker,
    pub actions_unpacker: ActionsUnpacker,

    pub last_process_time: Instant,

    pub remove_queue: EntityRemoveQueue,
}

impl SharedData {
    pub fn remove_entities(&mut self) {
        let mut remove_queue = self.remove_queue.take();

        if remove_queue.is_not_empty {
            for actor in remove_queue.actors.drain() {
                self.remove_actor(&actor);
            }
            for player in remove_queue.players.drain() {
                self.remove_player(&player);
            }

            remove_queue.is_not_empty = false;
        }

        self.remove_queue.return_taken(remove_queue);
    }

    pub fn remove_actor(&mut self, actor: &Actor) {
        self.class_ac.remove(actor, self.snapshot);
        self.position_ac.remove(actor, self.snapshot);
        self.velocity_ac.remove(actor, self.snapshot);
        self.orientation_ac.remove(actor, self.snapshot);
        self.player_ac.remove(actor);
        self.chunk_activation_ac.remove(actor);
        self.actor_registry.remove(actor);
    }

    pub fn prune_chunks(&mut self) {
        let retain = |chunk: &Chunk| self.chunk_activation_system.is_active(chunk);

        self.status_cc.retain(|chunk, status| {
            let retain = retain(chunk) || *status == ChunkStatus::Loading;

            if !retain {
                self.cache_cc.remove(chunk);
                self.class_bc.remove_chunk(chunk);
            }

            retain
        });
    }

    pub fn remove_player(&mut self, player: &Player) {
        self.client_pc.remove(&player);
        self.chunk_update_pc.remove(&player);
        self.chunk_view_pc.remove(&player);
        self.actions_packer_pc.remove(&player);
        if let Some(actor) = self.actor_pc.remove(&player) {
            self.remove_actor(&actor);
        }
    }

    pub fn add_player(&mut self, player: Player, tx: Sender<ClientEvent>, session_id: u64) {
        let tx_init = tx.clone();
        let actor = self.actor_registry.add();

        self.class_ac.insert(
            actor,
            self.actor_class_label_map.get("human").unwrap(),
            self.snapshot,
        );

        self.player_ac.insert(actor, player);

        self.chunk_activation_ac.insert(
            actor,
            ActorChunkActivation {
                radius: PLAYER_CHUNK_VIEW_RADIUS,
            },
        );

        self.client_pc.insert(
            player,
            Client {
                tx,
                last_server_snapshot: Snapshot(0),
                last_client_snapshot: Snapshot(0),
                last_confirmed_chunk: None,
                session_id,
            },
        );

        self.actor_pc.insert(player, actor);

        self.chunk_view_pc.insert(
            player,
            ChunkView {
                radius: PLAYER_CHUNK_VIEW_RADIUS,
            },
        );

        self.actions_packer_pc.insert(player, ActionsPacker::new());

        if tx_init.send(ClientEvent::AssignActor { actor }).is_err() {
            self.remove_player(&player);
        }
    }

    pub fn chunk_loaded(&mut self, chunk_data: ChunkData, data_encoded: Arc<Vec<u8>>) {
        match self.status_cc.get_mut(&chunk_data.chunk) {
            Some(status) if *status == ChunkStatus::Loading => {
                *status = ChunkStatus::Active;
            },
            _ => return,
        }

        self.class_bc
            .insert_chunk(chunk_data.chunk, chunk_data.block_classes);
        self.cache_cc
            .insert(chunk_data.chunk, data_encoded.clone().into());

        let chunk = chunk_data.chunk;

        for (player, client) in self.actor_pc.iter().filter_map(|(player, actor)| {
            let position = self.position_ac.get(actor)?;
            let chunk_ticket = self.chunk_activation_ac.get(actor)?;

            if position.chunk.radius(chunk_ticket.radius).is_within(&chunk) {
                Some((player, self.client_pc.get(player)?))
            } else {
                None
            }
        }) {
            if client
                .tx
                .send(ClientEvent::SendDataReliable {
                    channel: BASE_CHANNEL,
                    data: SendData::Arc(data_encoded.clone()),
                })
                .is_err()
            {
                self.remove_queue.remove_player(player);
            }
        }
    }
}
