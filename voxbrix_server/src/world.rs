use crate::{
    component::{
        actor::{
            chunk_ticket::ChunkTicketActorComponent,
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            player::PlayerActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::model::ModelActorClassComponent,
        chunk::{
            cache::CacheChunkComponent,
            status::{
                ChunkStatus,
                StatusChunkComponent,
            },
        },
        player::{
            actor::ActorPlayerComponent,
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
    server::SendRc,
    storage::StorageThread,
    system::{
        chunk_ticket::ChunkTicketSystem,
        position::PositionSystem,
    },
    Local,
    Shared,
    BASE_CHANNEL,
};
use local_channel::mpsc::Sender;
use nohash_hasher::IntSet;
use std::time::Instant;
use voxbrix_common::{
    component::{
        block::class::ClassBlockComponent,
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::{
        actor::Actor,
        actor_class::ActorClass,
        block_class::BlockClass,
        snapshot::Snapshot,
    },
    messages::StatePacker,
    pack::Packer,
    ChunkData,
    LabelMap,
};

mod player_event;
mod process;

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

    fn remove_actor(&mut self, actor: &Actor) {
        self.actors.insert(*actor);
        self.is_not_empty = true;
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

    pub fn remove_actor(&mut self, actor: &Actor) {
        self.0
            .as_mut()
            .expect("EntityRemoveQueue is taken")
            .remove_actor(actor)
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

/// All components and systems the loop has.
pub struct World {
    pub local: &'static Local,
    pub shared: &'static Shared,

    pub packer: Packer,
    pub actor_registry: ActorRegistry,

    pub client_pc: ClientPlayerComponent,
    pub actor_pc: ActorPlayerComponent,

    pub class_ac: ClassActorComponent,
    pub position_ac: PositionActorComponent,
    pub velocity_ac: VelocityActorComponent,
    pub orientation_ac: OrientationActorComponent,
    pub player_ac: PlayerActorComponent,
    pub chunk_ticket_ac: ChunkTicketActorComponent,

    pub model_acc: ModelActorClassComponent,

    pub class_bc: ClassBlockComponent,
    pub collision_bcc: CollisionBlockClassComponent,

    pub status_cc: StatusChunkComponent,
    pub cache_cc: CacheChunkComponent,

    pub actor_class_label_map: LabelMap<ActorClass>,
    pub block_class_label_map: LabelMap<BlockClass>,

    pub position_system: PositionSystem,
    pub chunk_ticket_system: ChunkTicketSystem,

    pub storage: StorageThread,

    pub snapshot: Snapshot,

    pub server_state: StatePacker,

    pub last_process_time: Instant,

    pub remove_queue: EntityRemoveQueue,
}

impl World {
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
        self.chunk_ticket_ac.remove(actor);
    }

    pub fn remove_player(&mut self, player: &Player) {
        self.client_pc.remove(&player);
        if let Some(actor) = self.actor_pc.remove(&player) {
            self.remove_actor(&actor);
        }
    }

    pub fn add_player(&mut self, player: Player, tx: Sender<ClientEvent>) {
        let tx_init = tx.clone();
        let actor = self.actor_registry.add();

        self.class_ac.insert(
            actor,
            self.actor_class_label_map.get("human").unwrap(),
            self.snapshot,
        );

        self.player_ac.insert(actor, player);

        self.client_pc.insert(
            player,
            Client {
                tx,
                last_server_snapshot: Snapshot(0),
                last_client_snapshot: Snapshot(0),
                last_confirmed_chunk: None,
            },
        );
        self.actor_pc.insert(player, actor);

        if tx_init.send(ClientEvent::AssignActor { actor }).is_err() {
            self.remove_player(&player);
        }
    }

    pub fn chunk_loaded(&mut self, chunk_data: ChunkData, data_encoded: SendRc<Vec<u8>>) {
        let chunk_data_buf = data_encoded.extract();
        match self.status_cc.get_mut(&chunk_data.chunk) {
            Some(status) if *status == ChunkStatus::Loading => {
                *status = ChunkStatus::Active;
            },
            _ => return,
        }

        self.class_bc
            .insert_chunk(chunk_data.chunk, chunk_data.block_classes);
        self.cache_cc
            .insert(chunk_data.chunk, chunk_data_buf.clone());

        let chunk = chunk_data.chunk;

        for (player, client) in self.actor_pc.iter().filter_map(|(player, actor)| {
            let chunk_ticket = self.chunk_ticket_ac.get(actor)?;
            if chunk_ticket
                .chunk
                .radius(chunk_ticket.radius)
                .is_within(&chunk)
            {
                Some((player, self.client_pc.get(player)?))
            } else {
                None
            }
        }) {
            if client
                .tx
                .send(ClientEvent::SendDataReliable {
                    channel: BASE_CHANNEL,
                    data: SendData::Ref(chunk_data_buf.clone()),
                })
                .is_err()
            {
                self.remove_queue.remove_player(player);
            }
        }
    }
}
