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
        chunk::{
            render_data::RenderDataChunkComponent,
            sky_light_data::SkyLightDataChunkComponent,
        },
        texture::location::LocationTextureComponent,
    },
    resource::{
        chunk_calculation_data::ChunkCalculationData,
        confirmed_snapshots::ConfirmedSnapshots,
        interface::Interface,
        interface_state::InterfaceState,
        player_actor::PlayerActor,
        player_chunk_view_radius::PlayerChunkViewRadius,
        player_input::PlayerInput,
        render_pool::{
            camera::CameraParameters,
            RenderPool,
            RenderPoolDescriptor,
        },
        server_sender::ServerSender,
    },
    scene::{
        menu::MenuSceneParameters,
        SceneSwitch,
    },
    system::{
        actor_render::ActorRenderSystemDescriptor,
        block_render::BlockRenderSystemDescriptor,
        entity_removal::{
            EntityRemovalCheckSystem,
            EntityRemovalSystem,
        },
        model_loading::ModelLoadingSystem,
        send_changes::SendChangesSystem,
        sky_light::SkyLightSystem,
        target_block_highlight::TargetBlockHightlightSystemDescriptor,
        texture_loading::TextureLoadingSystem,
    },
    window::{
        Frame,
        InputEvent,
        Window,
    },
    CONNECTION_TIMEOUT,
};
use anyhow::{
    Context,
    Result,
};
use chunk_calculation::ChunkCalculation;
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
use std::{
    io::ErrorKind as StdIoErrorKind,
    task::Poll,
    time::Duration,
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
    resource::{
        process_timer::ProcessTimer,
        removal_queue::RemovalQueue,
    },
    system::{
        actor_class_loading::ActorClassLoadingSystem,
        block_class_loading::BlockClassLoadingSystem,
        list_loading::List,
    },
    LabelLibrary,
};
use voxbrix_protocol::client::{
    Error as ClientError,
    Receiver,
    Sender,
};
use voxbrix_world::{
    System,
    SystemData,
    World,
};

mod chunk_calculation;
mod local_input;
mod network_input;
mod process;

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

        let (_reliable_tx, reliable_rx) = flume::unbounded::<Vec<u8>>();
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

        let player_input = PlayerInput::new(10.0, 0.4);
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

        let (block_texture_bind_group_layout, block_texture_bind_group) =
            block_texture_loading_system
                .prepare_buffer(
                    window.device(),
                    window.queue(),
                    BLOCK_TEXTURE_PATH_PREFIX,
                    &block_location_tc,
                )
                .await
                .context("unable to prepare block texture buffer")?;

        let (actor_texture_bind_group_layout, actor_texture_bind_group) =
            actor_texture_loading_system
                .prepare_buffer(
                    window.device(),
                    window.queue(),
                    ACTOR_TEXTURE_PATH_PREFIX,
                    &actor_location_tc,
                )
                .await
                .context("unable to prepare actor texture buffer")?;

        let interface = Interface::new();

        let player_position = position_ac
            .get(&player_actor)
            .expect("player position is undefined");

        let player_orientation = orientation_ac
            .get(&player_actor)
            .expect("player orientation is undefined");

        let render_pool = RenderPoolDescriptor {
            // TODO hide?
            camera_parameters: CameraParameters {
                chunk: player_position.chunk.position,
                offset: player_position.offset.into(),
                view_direction: player_orientation.forward().into(),
                aspect: 1.0,
                fovy: 70f32.to_radians(),
            },
            window,
        }
        .build();

        let window = render_pool.window();

        let render_parameters = render_pool.get_render_parameters();

        let block_render_system = BlockRenderSystemDescriptor {
            render_parameters,
            block_texture_bind_group_layout: block_texture_bind_group_layout.clone(),
            block_texture_bind_group: block_texture_bind_group.clone(),
        }
        .build(window)
        .await;

        let target_block_highlight_system = TargetBlockHightlightSystemDescriptor {
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

        let mut label_library = LabelLibrary::new();
        label_library.add(block_class_label_map);

        let frame_source = window.get_frame_source();
        let input_source = window.get_input_source();

        let mut world = World::new();

        world.add(packer);

        world.add(label_library);

        world.add(class_ac);
        world.add(position_ac);
        world.add(velocity_ac);
        world.add(orientation_ac);
        world.add(animation_state_ac);
        world.add(target_position_ac);
        world.add(target_orientation_ac);

        world.add(builder_amc);

        world.add(model_acc);

        world.add(class_bc);
        world.add(sky_light_bc);

        world.add(collision_bcc);
        world.add(model_bcc);
        world.add(opacity_bcc);

        world.add(status_cc);
        world.add(RenderDataChunkComponent::new());
        world.add(SkyLightDataChunkComponent::new());

        world.add(builder_bmc);
        world.add(culling_bmc);

        world.add(player_input);
        world.add(sky_light_system);
        world.add(interface);
        world.add(render_pool);
        world.add(actor_render_system);
        world.add(block_render_system);
        world.add(target_block_highlight_system);

        world.add(PlayerActor(player_actor));
        world.add(PlayerChunkViewRadius(player_chunk_view_radius));

        world.add(snapshot);
        world.add(ConfirmedSnapshots {
            last_client_snapshot,
            last_server_snapshot,
        });

        world.add(ServerSender {
            unreliable: unreliable_tx,
        });

        world.add(state_packer);
        world.add(StateUnpacker::new());
        world.add(ActionsPacker::new());
        world.add(ActionsUnpacker::new());

        world.add(ProcessTimer::new());

        world.add(ChunkCalculationData { turn: 0 });

        world.add(InterfaceState {
            inventory_open: false,
            cursor_visible: false,
        });

        world.add(RemovalQueue::<Actor>::new());

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
                struct ChunkCalculationCheck;

                impl System for ChunkCalculationCheck {
                    type Data<'a> = ChunkCalculationCheckData<'a>;
                }

                #[derive(SystemData)]
                struct ChunkCalculationCheckData<'a> {
                    sky_light_data_cc: &'a SkyLightDataChunkComponent,
                    render_data_cc: &'a RenderDataChunkComponent,
                }

                let data = world.get_data::<ChunkCalculationCheck>();

                // This works because the only update can come from the previous iteration of the
                // loop
                if data.sky_light_data_cc.is_queue_empty() && data.render_data_cc.is_queue_empty() {
                    return Poll::Pending;
                }

                Poll::Ready(Some(Event::ChunkCalculation))
            }))
            .await
        {
            if world.get_data::<EntityRemovalCheckSystem>().run() {
                world.get_data::<EntityRemovalSystem>().run();
            }

            let transition = match event {
                Event::Process(frame) => {
                    compute!((world) Process {
                    world: &mut world,
                    frame,
                }.run())
                },
                Event::SendState => {
                    world.get_data::<SendChangesSystem>().run();

                    Transition::None
                },
                Event::LocalInput(event) => {
                    LocalInput {
                        world: &mut world,
                        event,
                    }
                    .run()
                },
                Event::NetworkInput(event) => {
                    NetworkInput {
                        world: &mut world,
                        event,
                    }
                    .run()
                },
                Event::ChunkCalculation => ChunkCalculation { world: &mut world }.run(),
            };

            match transition {
                Transition::None => {},
                Transition::Exit => {
                    return Ok(SceneSwitch::Exit);
                },
                Transition::Menu => {
                    return Ok(SceneSwitch::Menu {
                        parameters: MenuSceneParameters {
                            window: world.take_resource::<RenderPool>().into_window(),
                        },
                    });
                },
            }
        }

        Ok(SceneSwitch::Menu {
            parameters: MenuSceneParameters {
                window: world.take_resource::<RenderPool>().into_window(),
            },
        })
    }
}
