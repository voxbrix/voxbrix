use crate::{
    component::{
        actor::{
            chunk_activation::{
                ActorChunkActivation,
                ChunkActivationActorComponent,
            },
            class::ClassActorComponent,
            player::PlayerActorComponent,
        },
        player::{
            actions_packer::ActionsPackerPlayerComponent,
            actor::ActorPlayerComponent,
            chunk_view::{
                ChunkView,
                ChunkViewPlayerComponent,
            },
            client::{
                Client,
                ClientEvent,
                ClientPlayerComponent,
            },
        },
    },
    entity::{
        actor::ActorRegistry,
        player::Player,
    },
    PLAYER_CHUNK_VIEW_RADIUS,
};
use flume::Sender;
use voxbrix_common::{
    entity::{
        actor_class::ActorClass,
        snapshot::Snapshot,
    },
    messages::ActionsPacker,
    resource::removal_queue::RemovalQueue,
    LabelLibrary,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PlayerAddSystem;

impl System for PlayerAddSystem {
    type Data<'a> = PlayerAddSystemData<'a>;
}

pub struct PlayerAddData {
    pub player: Player,
    pub tx: Sender<ClientEvent>,
    pub session_id: u64,
}

#[derive(SystemData)]
pub struct PlayerAddSystemData<'a> {
    snapshot: &'a Snapshot,

    label_library: &'a LabelLibrary,

    actor_pc: &'a mut ActorPlayerComponent,
    client_pc: &'a mut ClientPlayerComponent,
    chunk_view_pc: &'a mut ChunkViewPlayerComponent,
    actions_packer_pc: &'a mut ActionsPackerPlayerComponent,

    actor_registry: &'a mut ActorRegistry,
    class_ac: &'a mut ClassActorComponent,
    player_ac: &'a mut PlayerActorComponent,
    chunk_activation_ac: &'a mut ChunkActivationActorComponent,

    player_rq: &'a mut RemovalQueue<Player>,
}

impl PlayerAddSystemData<'_> {
    pub fn run(self, data: PlayerAddData) {
        let PlayerAddData {
            player,
            tx,
            session_id,
        } = data;

        let tx_init = tx.clone();
        let actor = self.actor_registry.add(*self.snapshot);

        self.class_ac.insert(
            actor,
            self.label_library.get::<ActorClass>("human").unwrap(),
            *self.snapshot,
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
            self.player_rq.enqueue(player);
        }
    }
}
