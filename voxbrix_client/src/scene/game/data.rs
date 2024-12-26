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
        block::class::ClassBlockComponent,
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
        controller::DirectControl,
        interface::InterfaceSystem,
        movement_interpolation::MovementInterpolationSystem,
        player_position::PlayerPositionSystem,
        render::RenderSystem,
    },
};
use flume::Sender;
use std::time::Instant;
use voxbrix_common::{
    component::{
        block::sky_light::SkyLightBlockComponent,
        block_class::{
            collision::CollisionBlockClassComponent,
            opacity::OpacityBlockClassComponent,
        },
        chunk::status::StatusChunkComponent,
    },
    entity::{
        actor::Actor,
        block_class::BlockClass,
        snapshot::Snapshot,
    },
    messages::{
        ActionsPacker,
        ActionsUnpacker,
        StatePacker,
        StateUnpacker,
    },
    pack::Packer,
    system::sky_light::SkyLightSystem,
    LabelMap,
};

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

    pub block_class_label_map: LabelMap<BlockClass>,

    // pub action_label_map: LabelMap<Action>,
    pub player_actor: Actor,
    pub player_chunk_view_radius: i32,

    pub snapshot: Snapshot,
    pub last_client_snapshot: Snapshot,
    pub last_server_snapshot: Snapshot,

    pub unreliable_tx: Sender<Vec<u8>>,
    #[allow(dead_code)]
    pub reliable_tx: Sender<Vec<u8>>,
    #[allow(dead_code)]
    pub event_tx: Sender<Event>,

    pub state_packer: StatePacker,
    pub state_unpacker: StateUnpacker,
    pub actions_packer: ActionsPacker,
    pub actions_unpacker: ActionsUnpacker,

    pub last_process_time: Instant,

    pub inventory_open: bool,
    pub cursor_visible: bool,
}
