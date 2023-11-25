use crate::{
    assets::{
        ACTOR_MODEL_ANIMATION_LIST_PATH,
        ACTOR_MODEL_BODY_PART_LIST_PATH,
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
    },
    entity::{
        actor_model::{
            ActorAnimation,
            ActorBodyPart,
        },
        block_model::BlockModel,
    },
    scene::{
        menu::MenuSceneParameters,
        SceneSwitch,
    },
    system::{
        actor_render::ActorRenderSystemDescriptor,
        block_render::BlockRenderSystemDescriptor,
        chunk_presence::ChunkPresenceSystem,
        chunk_render_pipeline::ChunkRenderPipelineSystem,
        controller::DirectControl,
        interface::InterfaceSystemDescriptor,
        model_loading::ModelLoadingSystem,
        movement_interpolation::MovementInterpolationSystem,
        player_position::PlayerPositionSystem,
        render::{
            camera::CameraParameters,
            output_thread::{
                OutputBundle,
                OutputThread,
            },
            RenderSystemDescriptor,
        },
        sky_light::SkyLightSystem,
        texture_loading::TextureLoadingSystem,
    },
    window::InputEvent,
    CONNECTION_TIMEOUT,
};
use anyhow::Result;
use data::{
    EntityRemoveQueue,
    GameSharedData,
};
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
        block::{
            class::ClassBlockComponent,
            sky_light::SkyLightBlockComponent,
        },
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
        actor_model::ActorModel,
        chunk::{
            Chunk,
            Dimension,
        },
        snapshot::Snapshot,
        state_component::StateComponent,
    },
    math::Vec3F32,
    messages::StatePacker,
    pack::Packer,
    system::{
        actor_class_loading::ActorClassLoadingSystem,
        block_class_loading::BlockClassLoadingSystem,
        list_loading::List,
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
    Process(OutputBundle),
    SendState,
    LocalInput(InputEvent),
    NetworkInput(Result<Vec<u8>, ClientError>),
}

#[must_use = "must be handled"]
enum Transition {
    None,
    Exit,
    Menu,
}

pub struct GameSceneParameters {
    pub interface_state: egui_winit::State,
    pub output_thread: OutputThread,
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
                    interface_state,
                    output_thread,
                    connection,
                    player_actor,
                    player_chunk_view_radius,
                },
        } = self;

        let (reliable_tx, reliable_rx) = flume::unbounded::<Vec<u8>>();
        let (unreliable_tx, unreliable_rx) = flume::unbounded::<Vec<u8>>();
        let (event_tx, event_rx) = flume::unbounded::<Event>();

        let client_state = StatePacker::new();

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

        let block_class_loading_system = BlockClassLoadingSystem::load_data().await?;
        let block_texture_loading_system =
            TextureLoadingSystem::load_data(BLOCK_TEXTURE_LIST_PATH, BLOCK_TEXTURE_PATH_PREFIX)
                .await?;

        let mut builder_bmc = BuilderBlockModelComponent::new();
        let mut culling_bmc = CullingBlockModelComponent::new();

        let block_model_loading_system =
            ModelLoadingSystem::load_data(BLOCK_MODEL_LIST_PATH, BLOCK_MODEL_PATH_PREFIX).await?;

        let block_model_context = BlockModelContext {
            block_texture_label_map: &block_texture_loading_system.label_map,
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

        let block_model_label_map =
            block_model_loading_system.into_label_map(BlockModel::from_usize);

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
        let actor_texture_loading_system =
            TextureLoadingSystem::load_data(ACTOR_TEXTURE_LIST_PATH, ACTOR_TEXTURE_PATH_PREFIX)
                .await?;

        let state_components_label_map = List::load(STATE_COMPONENTS_PATH)
            .await?
            .into_label_map(StateComponent::from_usize);

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

        let actor_body_part_label_map = List::load(ACTOR_MODEL_BODY_PART_LIST_PATH)
            .await?
            .into_label_map(ActorBodyPart::from_usize);
        let actor_animation_label_map = List::load(ACTOR_MODEL_ANIMATION_LIST_PATH)
            .await?
            .into_label_map(ActorAnimation::from_usize);

        let ctx = ActorModelBuilderContext {
            actor_texture_label_map: &actor_texture_loading_system.label_map,
            actor_body_part_label_map: &actor_body_part_label_map,
            actor_animation_label_map: &actor_animation_label_map,
        };

        actor_model_loading_system.load_component(
            "builder",
            &mut builder_amc,
            |desc: ActorModelBuilderDescriptor| desc.describe(&ctx),
        )?;

        let actor_model_label_map =
            actor_model_loading_system.into_label_map(ActorModel::from_usize);

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
                    dimension: Dimension { index: 0 },
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

        output_thread
            .window()
            .set_cursor_grab(winit::window::CursorGrabMode::Confined)
            .or_else(|_| {
                output_thread
                    .window()
                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
            })?;
        output_thread.window().set_cursor_visible(false);

        let texture_format = output_thread.current_surface_config().format;

        let (block_texture_bind_group_layout, block_texture_bind_group) =
            block_texture_loading_system.prepare_buffer(
                &output_thread.device(),
                &output_thread.queue(),
                &texture_format,
            );

        let (actor_texture_bind_group_layout, actor_texture_bind_group) =
            actor_texture_loading_system.prepare_buffer(
                &output_thread.device(),
                &output_thread.queue(),
                &texture_format,
            );

        let surface_size = output_thread.window().inner_size();

        let interface_system = InterfaceSystemDescriptor {
            state: interface_state,
            output_thread: &output_thread,
        }
        .build();

        let render_system = RenderSystemDescriptor {
            player_actor,
            // TODO hide?
            camera_parameters: CameraParameters {
                aspect: (surface_size.width as f32) / (surface_size.height as f32),
                fovy: 70f32.to_radians(),
                near: 0.01,
                far: 100.0,
            },
            position_ac: &position_ac,
            orientation_ac: &orientation_ac,
            output_thread,
        }
        .build();

        let output_thread = render_system.output_thread();

        let render_parameters = render_system.get_render_parameters();

        let block_render_system = BlockRenderSystemDescriptor {
            render_parameters,
            block_texture_bind_group_layout,
            block_texture_bind_group,
        }
        .build(&output_thread)
        .await;

        let actor_render_system = ActorRenderSystemDescriptor {
            render_parameters,
            actor_texture_bind_group_layout,
            actor_texture_bind_group,
        }
        .build(&output_thread)
        .await;

        let surface_source = output_thread.get_surface_source();
        let input_source = output_thread.get_input_source();

        let mut shared_data = GameSharedData {
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
            chunk_render_pipeline_system: ChunkRenderPipelineSystem::new(),

            block_class_label_map,

            player_actor,
            player_chunk_view_radius,

            snapshot,
            last_client_snapshot,
            last_server_snapshot,

            unreliable_tx,
            reliable_tx,
            event_tx,

            client_state,

            last_process_time,

            remove_queue: EntityRemoveQueue::new(),

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
            surface_source
                .stream()
                .map(|surface| Event::Process(surface))
                .rr_ff(event_rx.stream()),
        );

        while let Some(event) = stream.next().await {
            let transition = match event {
                Event::Process(output_bundle) => {
                    compute!((shared_data) Process {
                    shared_data: &mut shared_data,
                    output_bundle,
                }.run())
                },
                Event::SendState => {
                    SendState {
                        shared_data: &mut shared_data,
                    }
                    .run()
                },
                Event::LocalInput(event) => {
                    LocalInput {
                        shared_data: &mut shared_data,
                        event,
                    }
                    .run()
                },
                Event::NetworkInput(event) => {
                    NetworkInput {
                        shared_data: &mut shared_data,
                        event,
                    }
                    .run()
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
                            interface_state: shared_data.interface_system.into_interface_state(),
                            output_thread: shared_data.render_system.into_output(),
                        },
                    });
                },
            }
        }

        Ok(SceneSwitch::Menu {
            parameters: MenuSceneParameters {
                interface_state: shared_data.interface_system.into_interface_state(),
                output_thread: shared_data.render_system.into_output(),
            },
        })
    }
}
