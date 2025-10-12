use crate::{
    assets::{
        ACTOR_MODEL_ANIMATION_LIST_PATH,
        ACTOR_MODEL_BONE_LIST_PATH,
        ACTOR_MODEL_DIR,
        BLOCK_MODEL_DIR,
        BLOCK_MODEL_LIST_PATH,
    },
    component::{
        actor::{
            animation_state::AnimationStateActorComponent,
            class::ClassActorComponent,
            effect::EffectActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            target_orientation::TargetOrientationActorComponent,
            target_position::TargetPositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_class::{
            block_collision::BlockCollisionActorClassComponent,
            model::ModelActorClassComponent,
        },
        actor_model::builder::{
            ActorModelBuilderDescriptor,
            BuilderActorModelComponent,
        },
        block::class::ClassBlockComponent,
        block_class::model::ModelBlockClassComponent,
        block_model::{
            builder::{
                BlockModelBuilderDescriptor,
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
    },
    entity::{
        actor_model::{
            ActorAnimation,
            ActorBone,
        },
        block_model::BlockModel,
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
            CameraParameters,
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
use tokio::{
    task,
    time::{
        self,
        MissedTickBehavior,
    },
};
use voxbrix_common::{
    assets::{
        ACTOR_CLASS_DIR,
        ACTOR_CLASS_LIST_PATH,
        ACTOR_MODEL_LIST_PATH,
        BLOCK_CLASS_DIR,
        BLOCK_CLASS_LIST_PATH,
        UPDATE_LIST_PATH,
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
        actor_class::block_collision::BlockCollision,
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
        actor_class::ActorClass,
        actor_model::ActorModel,
        block_class::BlockClass,
        chunk::{
            Chunk,
            Dimension,
        },
        snapshot::{
            ClientSnapshot,
            ServerSnapshot,
        },
        update::Update,
    },
    math::Vec3F32,
    messages::{
        ClientActionsPacker,
        DispatchesUnpacker,
        UpdatesPacker,
        UpdatesUnpacker,
    },
    pack::Packer,
    resource::{
        process_timer::ProcessTimer,
        removal_queue::RemovalQueue,
    },
    system::component_map::ComponentMap,
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

        let mut label_library = LabelLibrary::new();

        label_library
            .load::<ActorClass>(ACTOR_CLASS_LIST_PATH)
            .await
            .context("ActorClass list loading error")?;

        label_library
            .load::<ActorModel>(ACTOR_MODEL_LIST_PATH)
            .await
            .context("ActorModel list loading error")?;

        label_library
            .load::<ActorBone>(ACTOR_MODEL_BONE_LIST_PATH)
            .await
            .context("ActorBone list loading error")?;

        label_library
            .load::<ActorAnimation>(ACTOR_MODEL_ANIMATION_LIST_PATH)
            .await
            .context("ActorAnimation list loading error")?;

        label_library
            .load::<BlockClass>(BLOCK_CLASS_LIST_PATH)
            .await
            .context("BlockClass list loading error")?;

        label_library
            .load::<BlockModel>(BLOCK_MODEL_LIST_PATH)
            .await
            .context("BlockModel list loading error")?;

        label_library
            .load::<Update>(UPDATE_LIST_PATH)
            .await
            .context("Update list loading error")?;

        let (_reliable_tx, reliable_rx) = flume::unbounded::<Vec<u8>>();
        let (unreliable_tx, unreliable_rx) = flume::unbounded::<Vec<u8>>();
        let (event_high_prio_tx, event_high_prio_rx) = flume::unbounded::<Event>();
        let (event_low_prio_tx, event_low_prio_rx) = flume::unbounded::<Event>();

        let updates_packer = UpdatesPacker::new();

        let snapshot = ClientSnapshot(1);
        // Last client's snapshot received by the server
        let last_client_snapshot = ClientSnapshot(0);
        let last_server_snapshot = ServerSnapshot(0);

        let packer = Packer::new();

        let (tx, mut rx) = connection;

        let (mut unreliable, mut reliable) = tx.split();

        let _send_unrel_task = {
            let event_high_prio_tx = event_high_prio_tx.clone();

            async_ext::spawn_scoped(async move {
                while let Ok(msg) = unreliable_rx.recv_async().await {
                    let result = unreliable.send_unreliable(&msg).await;

                    if let Err(err) = result {
                        let _ = event_high_prio_tx.send(Event::NetworkInput(Err(err)));
                        break;
                    }

                    task::yield_now().await;
                }
            })
        };

        let _send_rel_task = {
            let event_high_prio_tx = event_high_prio_tx.clone();

            async_ext::spawn_scoped(async move {
                while let Ok(msg) = reliable_rx
                    .recv_async()
                    .or(async {
                        let _ =
                            future::zip(reliable.wait_complete(), future::pending::<()>()).await;
                        unreachable!();
                    })
                    .await
                {
                    // https://github.com/rust-lang/rust/issues/70142
                    let result =
                        match time::timeout(CONNECTION_TIMEOUT, reliable.send_reliable(&msg))
                            .await
                            .map_err(|_| ClientError::Io(StdIoErrorKind::TimedOut.into()))
                        {
                            Ok(Ok(ok)) => Ok(ok),
                            Ok(Err(err)) => Err(err),
                            Err(err) => Err(err),
                        };

                    if let Err(err) = result {
                        let _ = event_high_prio_tx.send(Event::NetworkInput(Err(err)));
                        break;
                    }

                    task::yield_now().await;
                }
            })
        };

        // Must be dropped when the loop ends
        let _recv_task = {
            let event_high_prio_tx = event_high_prio_tx.clone();
            let event_low_prio_tx = event_low_prio_tx.clone();

            async_ext::spawn_scoped(async move {
                loop {
                    let msg = match rx.recv().await {
                        Ok(m) => m,
                        Err(err) => {
                            let _ = event_high_prio_tx.send(Event::NetworkInput(Err(err)));
                            break;
                        },
                    };

                    let result = if msg.is_reliable() {
                        event_low_prio_tx.send(Event::NetworkInput(Ok(msg.data().to_vec())))
                    } else {
                        event_high_prio_tx.send(Event::NetworkInput(Ok(msg.data().to_vec())))
                    };

                    if result.is_err() {
                        break;
                    };

                    task::yield_now().await;
                }
            })
        };

        let actor_class_component_map =
            ComponentMap::<ActorClass>::load_data(ACTOR_CLASS_DIR, &label_library).await?;

        let actor_model_component_map =
            ComponentMap::<ActorModel>::load_data(ACTOR_MODEL_DIR, &label_library).await?;

        let block_class_component_map =
            ComponentMap::<BlockClass>::load_data(BLOCK_CLASS_DIR, &label_library).await?;

        let block_model_component_map =
            ComponentMap::<BlockModel>::load_data(BLOCK_MODEL_DIR, &label_library).await?;

        let texture_loading_system =
            TextureLoadingSystem::load_data(window.device(), window.queue()).await?;

        label_library.add_label_map(texture_loading_system.label_map());

        let builder_bmc = BuilderBlockModelComponent::new(
            &block_model_component_map,
            &label_library,
            "builder",
            |desc: BlockModelBuilderDescriptor| desc.describe(&label_library),
        )?;

        let culling_bmc = CullingBlockModelComponent::new(
            &block_model_component_map,
            &label_library,
            "culling",
            |value: Culling| Ok(value),
        )?;

        let status_cc = StatusChunkComponent::new();

        let class_bc = ClassBlockComponent::new();
        let sky_light_bc = SkyLightBlockComponent::new();

        let model_bcc = ModelBlockClassComponent::new(
            &block_class_component_map,
            &label_library,
            "model",
            |model_label: String| {
                label_library.get(&model_label).ok_or_else(|| {
                    anyhow::Error::msg(format!(
                        "block texture with label \"{}\" is undefined",
                        model_label
                    ))
                })
            },
        )?;

        let collision_bcc = CollisionBlockClassComponent::new(
            &block_class_component_map,
            &label_library,
            "collision",
            |desc: Collision| Ok(desc),
        )?;

        let opacity_bcc = OpacityBlockClassComponent::new(
            &block_class_component_map,
            &label_library,
            "opacity",
            |desc: Opacity| Ok(desc),
        )?;

        let player_input = PlayerInput::new(10.0, 0.4);
        let sky_light_system = SkyLightSystem::new();

        let class_ac = ClassActorComponent::new(
            label_library.get("actor_class").unwrap(),
            player_actor,
            false,
        );
        let effect_ac = EffectActorComponent::new(label_library.get("actor_effect").unwrap());
        let mut position_ac =
            PositionActorComponent::new(label_library.get("actor_position").unwrap(), player_actor);
        let mut velocity_ac = VelocityActorComponent::new(
            label_library.get("actor_velocity").unwrap(),
            player_actor,
            true,
        );
        let mut orientation_ac = OrientationActorComponent::new(
            label_library.get("actor_orientation").unwrap(),
            player_actor,
            true,
        );
        let animation_state_ac = AnimationStateActorComponent::new();
        let target_orientation_ac =
            TargetOrientationActorComponent::new(label_library.get("actor_orientation").unwrap());
        let target_position_ac =
            TargetPositionActorComponent::new(label_library.get("actor_position").unwrap());

        let model_acc = ModelActorClassComponent::new(
            label_library.get("actor_model").unwrap(),
            player_actor,
            false,
            &actor_class_component_map,
            &label_library,
            "model",
            |model_label: String| {
                label_library.get(&model_label).ok_or_else(|| {
                    anyhow::Error::msg(format!("actor model \"{}\" is undefined", model_label))
                })
            },
        )?;

        let block_collision_acc = BlockCollisionActorClassComponent::new(
            label_library.get("actor_block_collision").unwrap(),
            player_actor,
            false,
            &actor_class_component_map,
            &label_library,
            "block_collision",
            |desc: BlockCollision| Ok(desc),
        )?;

        let builder_amc = BuilderActorModelComponent::new(
            &actor_model_component_map,
            &label_library,
            "builder",
            |desc: ActorModelBuilderDescriptor| desc.describe(&label_library),
        )?;

        position_ac.insert(
            player_actor,
            Position {
                chunk: Chunk {
                    position: [0, 0, 0].into(),
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
                chunk: player_position.chunk.position.into(),
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
            block_texture_bind_group_layout: texture_loading_system.bind_group_layout(),
            block_texture_bind_group: texture_loading_system.bind_group(),
        }
        .build(window)
        .await;

        let target_block_highlight_system = TargetBlockHightlightSystemDescriptor {
            render_parameters,
            block_texture_bind_group_layout: texture_loading_system.bind_group_layout(),
            block_texture_bind_group: texture_loading_system.bind_group(),
            block_texture_label_map: texture_loading_system.label_map(),
        }
        .build(window)
        .await;

        let actor_render_system = ActorRenderSystemDescriptor {
            render_parameters,
            actor_texture_bind_group_layout: texture_loading_system.bind_group_layout(),
            actor_texture_bind_group: texture_loading_system.bind_group(),
        }
        .build(window)
        .await;

        let frame_source = window.get_frame_source();
        let input_source = window.get_input_source();

        let mut world = World::new();

        world.add(packer);

        world.add(label_library);

        world.add(class_ac);
        world.add(effect_ac);
        world.add(position_ac);
        world.add(velocity_ac);
        world.add(orientation_ac);
        world.add(animation_state_ac);
        world.add(target_position_ac);
        world.add(target_orientation_ac);

        world.add(builder_amc);

        world.add(block_collision_acc);
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

        world.add(updates_packer);
        world.add(UpdatesUnpacker::new());
        world.add(ClientActionsPacker::new());
        world.add(DispatchesUnpacker::new());

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
        .or_ff(event_high_prio_rx.stream())
        .or_ff(input_source.stream().map(Event::LocalInput))
        .or_ff(
            frame_source
                .stream()
                .map(|frame| Event::Process(frame))
                .rr_ff(event_low_prio_rx.stream()),
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

            task::yield_now().await;
        }

        Ok(SceneSwitch::Menu {
            parameters: MenuSceneParameters {
                window: world.take_resource::<RenderPool>().into_window(),
            },
        })
    }
}
