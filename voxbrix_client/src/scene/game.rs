use crate::{
    assets::{
        ACTOR_MODEL_ANIMATION_LIST_PATH,
        ACTOR_MODEL_BONE_LIST_PATH,
        ACTOR_MODEL_PATH_PREFIX,
        ACTOR_TEXTURE_LIST_PATH,
        ACTOR_TEXTURE_PATH_PREFIX,
        BLOCK_MODEL_LIST_PATH,
        BLOCK_MODEL_PATH_PREFIX,
        BLOCK_TEXTURE_LIST_PATH,
        BLOCK_TEXTURE_PATH_PREFIX,
    },
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
        actor_model::builder::{
            ActorModelBuilderContext,
            ActorModelBuilderDescriptor,
            BuilderActorModelComponent,
        },
        block::class::ClassBlockComponent,
        block_class::model::ModelBlockClassComponent,
        block_model::{
            builder::{
                BlockModelBuilderDescriptor,
                BlockModelContext,
                BuilderBlockModelComponent,
            },
            culling::{
                Culling,
                CullingBlockModelComponent,
            },
        },
        texture::location::LocationTextureComponent,
    },
    scene::{
        menu::MenuSceneParameters,
        SceneSwitch,
    },
    system::{
        actor_render::ActorRenderSystemDescriptor,
        block_render::BlockRenderSystemDescriptor,
        chunk_presence::ChunkPresenceSystem,
        controller::DirectControl,
        interface::InterfaceSystem,
        model_loading::ModelLoadingSystem,
        movement_interpolation::MovementInterpolationSystem,
        player_position::PlayerPositionSystem,
        render::{
            camera::CameraParameters,
            RenderSystemDescriptor,
        },
        texture_loading::TextureLoadingSystem,
    },
    window::{
        Frame,
        InputEvent,
        Window,
    },
    CONNECTION_TIMEOUT,
};
use anyhow::Result;
use data::GameSharedData;
use futures_lite::{
    future::{
        self,
        FutureExt,
    },
    stream::{
        self,
        StreamExt,
    },
};
use local_input::LocalInput;
use network_input::NetworkInput;
use process::Process;
use send_state::SendState;
use std::{
    io::ErrorKind as StdIoErrorKind,
    task::Poll,
    time::{
        Duration,
        Instant,
    },
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
    async_ext::{
        self,
        StreamExt as _,
    },
    component::{
        actor::{
            orientation::Orientation,
            position::Position,
            velocity::Velocity,
        },
        block::sky_light::SkyLightBlockComponent,
        block_class::{
            collision::{
                Collision,
                CollisionBlockClassComponent,
            },
            opacity::{
                Opacity,
                OpacityBlockClassComponent,
            },
        },
        chunk::status::StatusChunkComponent,
    },
    compute,
    entity::{
        actor::Actor,
        chunk::{
            Chunk,
            Dimension,
        },
        snapshot::Snapshot,
    },
    math::Vec3F32,
    messages::{
        ActionsPacker,
        ActionsUnpacker,
        StatePacker,
        StateUnpacker,
    },
    pack::Packer,
    system::{
        actor_class_loading::ActorClassLoadingSystem,
        block_class_loading::BlockClassLoadingSystem,
        list_loading::List,
        sky_light::SkyLightSystem,
    },
};
use voxbrix_protocol::client::{
    Error as ClientError,
    Receiver,
    Sender,
};

mod data;
mod local_input;
mod network_input;
mod process;
mod send_state;

enum Event {
    Process(Frame),
    SendState,
    LocalInput(InputEvent),
    NetworkInput(Result<Vec<u8>, ClientError>),
    ChunkCalculation,
}

#[must_use = "must be handled"]
enum Transition {
    None,
    Exit,
    Menu,
}

pub struct GameSceneParameters {
    pub window: Window,
    pub connection: (Sender, Receiver),
    pub player_actor: Actor,
    pub player_chunk_view_radius: i32,
}

pub struct GameScene {
    pub parameters: GameSceneParameters,
}

impl GameScene {
    pub async fn run(self) -> Result<SceneSwitch> {
        let GameScene {
            parameters:
                GameSceneParameters {
                    mut window,
                    connection,
                    player_actor,
                    player_chunk_view_radius,
                },
        } = self;

        let (reliable_tx, reliable_rx) = flume::unbounded::<Vec<u8>>();
        let (unreliable_tx, unreliable_rx) = flume::unbounded::<Vec<u8>>();
        let (event_tx, event_rx) = flume::unbounded::<Event>();

        let state_packer = StatePacker::new();

        let snapshot = Snapshot(1);
        // Last client's snapshot received by the server
        let last_client_snapshot = Snapshot(0);
        let last_server_snapshot = Snapshot(0);

        let packer = Packer::new();

        let (tx, mut rx) = connection;

        let (mut unreliable, mut reliable) = tx.split();

        let _send_unrel_task = async_ext::spawn_scoped(async move {
            while let Ok(msg) = unreliable_rx.recv_async().await {
                unreliable
                    .send_unreliable(0, &msg)
                    .await
                    .expect("send_unreliable should not fail");
            }
        });

        let event_tx_network = event_tx.clone();

        let _send_rel_task = async_ext::spawn_scoped(async move {
            while let Ok(msg) = reliable_rx
                .recv_async()
                .or(async {
                    let _ = future::zip(reliable.wait_complete(), future::pending::<()>()).await;
                    unreachable!();
                })
                .await
            {
                // https://github.com/rust-lang/rust/issues/70142
                let result =
                    match time::timeout(CONNECTION_TIMEOUT, reliable.send_reliable(0, &msg))
                        .await
                        .map_err(|_| ClientError::Io(StdIoErrorKind::TimedOut.into()))
                    {
                        Ok(Ok(ok)) => Ok(ok),
                        Ok(Err(err)) => Err(err),
                        Err(err) => Err(err),
                    };

                if let Err(err) = result {
                    let _ = event_tx_network.send(Event::NetworkInput(Err(err)));
                    break;
                }
            }
        });

        let event_tx_network = event_tx.clone();

        // Must be dropped when the loop ends
        let _recv_task = async_ext::spawn_scoped(async move {
            loop {
                let data = match rx.recv().await {
                    Ok((_channel, data)) => data,
                    Err(err) => {
                        let _ = event_tx_network.send(Event::NetworkInput(Err(err)));
                        break;
                    },
                };

                if event_tx_network
                    .send(Event::NetworkInput(Ok(data.to_vec())))
                    .is_err()
                {
                    break;
                };
            }
        });

        let mut block_location_tc = LocationTextureComponent::new();

        let block_class_loading_system = BlockClassLoadingSystem::load_data().await?;
        let block_texture_loading_system = TextureLoadingSystem::load_data(
            window.device(),
            BLOCK_TEXTURE_LIST_PATH,
            BLOCK_TEXTURE_PATH_PREFIX,
            &mut block_location_tc,
        )
        .await?;

        let mut builder_bmc = BuilderBlockModelComponent::new();
        let mut culling_bmc = CullingBlockModelComponent::new();

        let block_model_loading_system =
            ModelLoadingSystem::load_data(BLOCK_MODEL_LIST_PATH, BLOCK_MODEL_PATH_PREFIX).await?;

        let block_model_context = BlockModelContext {
            texture_label_map: block_texture_loading_system.label_map(),
            location_tc: &block_location_tc,
        };

        block_model_loading_system.load_component(
            "builder",
            &mut builder_bmc,
            |desc: BlockModelBuilderDescriptor| desc.describe(&block_model_context),
        )?;

        block_model_loading_system.load_component(
            "culling",
            &mut culling_bmc,
            |value: Culling| Ok(value),
        )?;

        let status_cc = StatusChunkComponent::new();

        let class_bc = ClassBlockComponent::new();
        let sky_light_bc = SkyLightBlockComponent::new();

        let mut model_bcc = ModelBlockClassComponent::new();
        let mut collision_bcc = CollisionBlockClassComponent::new();
        let mut opacity_bcc = OpacityBlockClassComponent::new();

        let block_model_label_map = block_model_loading_system.into_label_map();

        block_class_loading_system.load_component(
            "model",
            &mut model_bcc,
            |model_label: String| {
                block_model_label_map.get(&model_label).ok_or_else(|| {
                    anyhow::Error::msg(format!(
                        "block texture with label \"{}\" is undefined",
                        model_label
                    ))
                })
            },
        )?;

        block_class_loading_system.load_component(
            "collision",
            &mut collision_bcc,
            |desc: Collision| Ok(desc),
        )?;

        block_class_loading_system.load_component(
            "opacity",
            &mut opacity_bcc,
            |desc: Opacity| Ok(desc),
        )?;

        let block_class_label_map = block_class_loading_system.into_label_map();

        let last_process_time = Instant::now();

        let player_position_system = PlayerPositionSystem::new(player_actor);
        let movement_interpolation_system = MovementInterpolationSystem::new();
        let direct_control_system = DirectControl::new(player_actor, 10.0, 0.4);
        let chunk_presence_system = ChunkPresenceSystem::new();
        let sky_light_system = SkyLightSystem::new();

        let mut actor_location_tc = LocationTextureComponent::new();
        let actor_texture_loading_system = TextureLoadingSystem::load_data(
            window.device(),
            ACTOR_TEXTURE_LIST_PATH,
            ACTOR_TEXTURE_PATH_PREFIX,
            &mut actor_location_tc,
        )
        .await?;

        let state_components_label_map = List::load(STATE_COMPONENTS_PATH).await?.into_label_map();

        let class_ac = ClassActorComponent::new(
            state_components_label_map.get("actor_class").unwrap(),
            player_actor,
            false,
        );
        let mut position_ac = PositionActorComponent::new(
            state_components_label_map.get("actor_position").unwrap(),
            player_actor,
            true,
        );
        let mut velocity_ac = VelocityActorComponent::new(
            state_components_label_map.get("actor_velocity").unwrap(),
            player_actor,
            true,
        );
        let mut orientation_ac = OrientationActorComponent::new(
            state_components_label_map.get("actor_orientation").unwrap(),
            player_actor,
            true,
        );
        let animation_state_ac = AnimationStateActorComponent::new();
        let target_orientation_ac = TargetOrientationActorComponent::new(
            state_components_label_map.get("actor_orientation").unwrap(),
        );
        let target_position_ac = TargetPositionActorComponent::new(
            state_components_label_map.get("actor_position").unwrap(),
        );

        let mut model_acc = ModelActorClassComponent::new(
            state_components_label_map.get("actor_model").unwrap(),
            player_actor,
            false,
        );

        let actor_class_loading_system = ActorClassLoadingSystem::load_data().await?;
        let actor_model_loading_system =
            ModelLoadingSystem::load_data(ACTOR_MODEL_LIST_PATH, ACTOR_MODEL_PATH_PREFIX).await?;
        let mut builder_amc = BuilderActorModelComponent::new();

        let actor_bone_label_map = List::load(ACTOR_MODEL_BONE_LIST_PATH)
            .await?
            .into_label_map();
        let actor_animation_label_map = List::load(ACTOR_MODEL_ANIMATION_LIST_PATH)
            .await?
            .into_label_map();

        let ctx = ActorModelBuilderContext {
            texture_label_map: actor_texture_loading_system.label_map(),
            location_tc: &actor_location_tc,
            actor_bone_label_map: &actor_bone_label_map,
            actor_animation_label_map: &actor_animation_label_map,
        };

        actor_model_loading_system.load_component(
            "builder",
            &mut builder_amc,
            |desc: ActorModelBuilderDescriptor| desc.describe(&ctx),
        )?;

        let actor_model_label_map = actor_model_loading_system.into_label_map();

        actor_class_loading_system.load_component(
            "model",
            &mut model_acc,
            |model_label: String| {
                actor_model_label_map.get(&model_label).ok_or_else(|| {
                    anyhow::Error::msg(format!("actor model \"{}\" is undefined", model_label))
                })
            },
        )?;

        let _actor_class_map = actor_class_loading_system.into_label_map();

        position_ac.insert(
            player_actor,
            Position {
                chunk: Chunk {
                    position: [0, 0, 0],
                    dimension: Dimension {
                        kind: voxbrix_common::entity::chunk::DimensionKind(0),
                        phase: 0,
                    },
                },
                offset: Vec3F32::new(0.0, 0.0, 4.0),
            },
            snapshot,
        );
        velocity_ac.insert(
            player_actor,
            Velocity {
                vector: Vec3F32::new(0.0, 0.0, 0.0),
            },
            snapshot,
        );
        orientation_ac.insert(
            player_actor,
            Orientation::from_yaw_pitch(0.0, 0.0),
            snapshot,
        );

        window.cursor_visible = false;

        let texture_format = window.texture_format();

        let (block_texture_bind_group_layout, block_texture_bind_group) =
            block_texture_loading_system.prepare_buffer(
                window.device(),
                window.queue(),
                &texture_format,
            );

        let (actor_texture_bind_group_layout, actor_texture_bind_group) =
            actor_texture_loading_system.prepare_buffer(
                window.device(),
                window.queue(),
                &texture_format,
            );

        let interface_system = InterfaceSystem::new();

        let render_system = RenderSystemDescriptor {
            player_actor,
            // TODO hide?
            camera_parameters: CameraParameters {
                aspect: 1.0,
                fovy: 70f32.to_radians(),
                near: 0.01,
                far: 100.0,
            },
            position_ac: &position_ac,
            orientation_ac: &orientation_ac,
            window,
        }
        .build();

        let window = render_system.window();

        let render_parameters = render_system.get_render_parameters();

        let block_render_system = BlockRenderSystemDescriptor {
            render_parameters,
            block_texture_bind_group_layout,
            block_texture_bind_group,
            block_texture_label_map: block_texture_loading_system.label_map(),
            location_tc: &block_location_tc,
        }
        .build(window)
        .await;

        let actor_render_system = ActorRenderSystemDescriptor {
            render_parameters,
            actor_texture_bind_group_layout,
            actor_texture_bind_group,
        }
        .build(window)
        .await;

        let frame_source = window.get_frame_source();
        let input_source = window.get_input_source();

        let mut chunk_calc_phase = 0;

        let mut sd = GameSharedData {
            packer,

            class_ac,
            position_ac,
            velocity_ac,
            orientation_ac,
            animation_state_ac,
            target_position_ac,
            target_orientation_ac,

            builder_amc,

            model_acc,

            class_bc,
            sky_light_bc,

            collision_bcc,
            model_bcc,
            opacity_bcc,

            status_cc,

            builder_bmc,
            culling_bmc,

            player_position_system,
            movement_interpolation_system,
            direct_control_system,
            chunk_presence_system,
            sky_light_system,
            interface_system,
            render_system,
            actor_render_system,
            block_render_system,

            block_class_label_map,

            player_actor,
            player_chunk_view_radius,

            snapshot,
            last_client_snapshot,
            last_server_snapshot,

            unreliable_tx,
            reliable_tx,
            event_tx,

            state_packer,
            state_unpacker: StateUnpacker::new(),
            actions_packer: ActionsPacker::new(),
            actions_unpacker: ActionsUnpacker::new(),

            last_process_time,

            inventory_open: false,
            cursor_visible: false,
        };

        let mut send_state_interval = time::interval(Duration::from_millis(50));
        send_state_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut stream = stream::poll_fn(|cx| {
            send_state_interval
                .poll_tick(cx)
                .map(|_| Some(Event::SendState))
        })
        .or_ff(input_source.stream().map(Event::LocalInput))
        .or_ff(
            frame_source
                .stream()
                .map(|frame| Event::Process(frame))
                .rr_ff(event_rx.stream()),
        );

        while let Some(event) = stream
            .next()
            .or(future::poll_fn(|_| {
                // This works because the only update can come from the previous iteration of the
                // loop
                if sd.sky_light_system.is_queue_empty() && sd.block_render_system.is_queue_empty() {
                    return Poll::Pending;
                }

                Poll::Ready(Some(Event::ChunkCalculation))
            }))
            .await
        {
            let transition = match event {
                Event::Process(frame) => {
                    compute!((sd) Process {
                    shared_data: &mut sd,
                    frame,
                }.run())
                },
                Event::SendState => {
                    SendState {
                        shared_data: &mut sd,
                    }
                    .run()
                },
                Event::LocalInput(event) => {
                    LocalInput {
                        shared_data: &mut sd,
                        event,
                    }
                    .run()
                },
                Event::NetworkInput(event) => {
                    NetworkInput {
                        shared_data: &mut sd,
                        event,
                    }
                    .run()
                },
                Event::ChunkCalculation => {
                    chunk_calc_phase = match chunk_calc_phase {
                        0 => {
                            let changed_chunks = sd.sky_light_system.process(
                                voxbrix_common::entity::block::BLOCKS_IN_CHUNK,
                                &sd.class_bc,
                                &sd.opacity_bcc,
                                &mut sd.sky_light_bc,
                            );

                            for chunk in changed_chunks {
                                sd.block_render_system.enqueue_chunk(chunk);
                            }

                            1
                        },
                        1 => {
                            sd.block_render_system.process(
                                &sd.class_bc,
                                &sd.model_bcc,
                                &sd.builder_bmc,
                                &sd.culling_bmc,
                                &sd.sky_light_bc,
                            );

                            0
                        },
                        _ => unreachable!(),
                    };

                    Transition::None
                },
            };

            match transition {
                Transition::None => {},
                Transition::Exit => {
                    return Ok(SceneSwitch::Exit);
                },
                Transition::Menu => {
                    return Ok(SceneSwitch::Menu {
                        parameters: MenuSceneParameters {
                            window: sd.render_system.into_window(),
                        },
                    });
                },
            }
        }

        Ok(SceneSwitch::Menu {
            parameters: MenuSceneParameters {
                window: sd.render_system.into_window(),
            },
        })
    }
}
