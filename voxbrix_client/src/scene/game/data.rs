use crate::{
    component::{
        actor::{
            animation_state::AnimationStateActorComponent,
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            target_orientation::TargetOrientationActorComponent,
            target_position::TargetPositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::model::ModelActorClassComponent,
        actor_model::builder::BuilderActorModelComponent,
        block_class::model::ModelBlockClassComponent,
        block_model::{
            builder::BuilderBlockModelComponent,
            culling::CullingBlockModelComponent,
        },
    },
    scene::game::Event,
    system::{
        actor_render::ActorRenderSystem,
        block_render::BlockRenderSystem,
        chunk_presence::ChunkPresenceSystem,
        chunk_render_pipeline::ChunkRenderPipelineSystem,
        controller::DirectControl,
        interface::InterfaceSystem,
        movement_interpolation::MovementInterpolationSystem,
        player_position::PlayerPositionSystem,
        render::RenderSystem,
        sky_light::SkyLightSystem,
    },
};
use flume::Sender;
use nohash_hasher::IntSet;
use std::time::Instant;
use voxbrix_common::{
    component::{
        block::{
            class::ClassBlockComponent,
            sky_light::SkyLightBlockComponent,
        },
        block_class::{
            collision::CollisionBlockClassComponent,
            opacity::OpacityBlockClassComponent,
        },
        chunk::status::StatusChunkComponent,
    },
    entity::{
        action::Action,
        actor::Actor,
        block_class::BlockClass,
        snapshot::Snapshot,
    },
    messages::{
        ActionsPacker,
        StatePacker,
        StateUnpacker,
    },
    pack::Packer,
    LabelMap,
};

pub struct EntityRemoveQueue(Option<EntityRemoveQueueInner>);

struct EntityRemoveQueueInner {
    is_not_empty: bool,
    actors: IntSet<Actor>,
}

impl EntityRemoveQueueInner {
    fn new() -> Option<Self> {
        Some(Self {
            is_not_empty: false,
            actors: IntSet::default(),
        })
    }

    fn remove_actor(&mut self, actor: &Actor) {
        self.actors.insert(*actor);
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

    fn take(&mut self) -> EntityRemoveQueueInner {
        self.0.take().expect("EntityRemoveQueue is taken")
    }

    fn return_taken(&mut self, taken: EntityRemoveQueueInner) {
        self.0 = Some(taken);
    }
}

/// All components and systems the loop has.
pub struct GameSharedData {
    pub packer: Packer,

    pub class_ac: ClassActorComponent,
    pub position_ac: PositionActorComponent,
    pub velocity_ac: VelocityActorComponent,
    pub orientation_ac: OrientationActorComponent,
    pub animation_state_ac: AnimationStateActorComponent,
    pub target_position_ac: TargetPositionActorComponent,
    pub target_orientation_ac: TargetOrientationActorComponent,

    pub builder_amc: BuilderActorModelComponent,

    pub model_acc: ModelActorClassComponent,

    pub class_bc: ClassBlockComponent,
    pub sky_light_bc: SkyLightBlockComponent,

    pub collision_bcc: CollisionBlockClassComponent,
    pub model_bcc: ModelBlockClassComponent,
    pub opacity_bcc: OpacityBlockClassComponent,

    pub status_cc: StatusChunkComponent,

    pub builder_bmc: BuilderBlockModelComponent,
    pub culling_bmc: CullingBlockModelComponent,

    pub player_position_system: PlayerPositionSystem,
    pub movement_interpolation_system: MovementInterpolationSystem,
    pub direct_control_system: DirectControl,
    pub chunk_presence_system: ChunkPresenceSystem,
    pub sky_light_system: SkyLightSystem,
    pub interface_system: InterfaceSystem,
    pub render_system: RenderSystem,
    pub actor_render_system: ActorRenderSystem,
    pub block_render_system: BlockRenderSystem,
    pub chunk_render_pipeline_system: ChunkRenderPipelineSystem,

    pub block_class_label_map: LabelMap<BlockClass>,

    // pub action_label_map: LabelMap<Action>,
    pub player_actor: Actor,
    pub player_chunk_view_radius: i32,

    pub snapshot: Snapshot,
    pub last_client_snapshot: Snapshot,
    pub last_server_snapshot: Snapshot,

    pub unreliable_tx: Sender<Vec<u8>>,
    pub reliable_tx: Sender<Vec<u8>>,
    pub event_tx: Sender<Event>,

    pub state_packer: StatePacker,
    pub state_unpacker: StateUnpacker,
    pub actions_packer: ActionsPacker,

    pub last_process_time: Instant,

    pub remove_queue: EntityRemoveQueue,

    pub inventory_open: bool,
    pub cursor_visible: bool,
}

impl GameSharedData {
    pub fn remove_entities(&mut self) {
        let mut remove_queue = self.remove_queue.take();

        if remove_queue.is_not_empty {
            for actor in remove_queue.actors.drain() {
                self.remove_actor(&actor);
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
        self.animation_state_ac.remove_actor(actor);
        self.target_position_ac.remove(actor);
        self.target_orientation_ac.remove(actor);
    }
}
