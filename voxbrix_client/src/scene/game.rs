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
            TargetQueue,
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
        texture_loading::TextureLoadingSystem,
    },
    window::InputEvent,
    CONNECTION_TIMEOUT,
};
use anyhow::Result;
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
use log::error;
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
        chunk::status::{
            ChunkStatus,
            StatusChunkComponent,
        },
    },
    entity::{
        actor::Actor,
        actor_model::ActorModel,
        block::Block,
        chunk::{
            Chunk,
            Dimension,
        },
        snapshot::Snapshot,
        state_component::StateComponent,
    },
    math::Vec3F32,
    messages::{
        client::ClientAccept,
        server::ServerAccept,
        StatePacker,
    },
    pack::Packer,
    system::{
        actor_class_loading::ActorClassLoadingSystem,
        block_class_loading::BlockClassLoadingSystem,
        list_loading::List,
        sky_light::SkyLightSystem,
    },
    unblock,
    ChunkData,
};
use voxbrix_protocol::client::{
    Error as ClientError,
    Receiver,
    Sender,
};
use winit::event::{
    DeviceEvent,
    ElementState,
    MouseButton,
    WindowEvent,
};

pub enum Event {
    Process(OutputBundle),
    SendState,
    Input(InputEvent),
    NetworkInput(Result<Vec<u8>, ClientError>),
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

        let (reliable_tx, mut reliable_rx) = local_channel::mpsc::channel::<Vec<u8>>();
        let (unreliable_tx, mut unreliable_rx) = local_channel::mpsc::channel::<Vec<u8>>();
        let (event_tx, event_rx) = local_channel::mpsc::channel::<Event>();

        let mut client_state = StatePacker::new();

        let mut snapshot = Snapshot(1);
        // Last client's snapshot received by the server
        let mut last_client_snapshot = Snapshot(0);
        let mut last_server_snapshot = Snapshot(0);

        let mut packer = Packer::new();

        let (tx, mut rx) = connection;

        let (mut unreliable, mut reliable) = tx.split();

        let _send_unrel_task = async_ext::spawn_scoped(async move {
            while let Some(msg) = unreliable_rx.recv().await {
                unreliable
                    .send_unreliable(0, &msg)
                    .await
                    .expect("send_unreliable should not fail");
            }
        });

        let event_tx_network = event_tx.clone();

        let _send_rel_task = async_ext::spawn_scoped(async move {
            while let Some(msg) = reliable_rx
                .recv()
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

        let mut status_cc = StatusChunkComponent::new();

        let mut class_bc = ClassBlockComponent::new();
        let mut sky_light_bc = SkyLightBlockComponent::new();

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

        let block_class_map = block_class_loading_system.into_label_map();

        let mut last_render_time = Instant::now();

        let mut player_position_system = PlayerPositionSystem::new(player_actor);
        let mut movement_interpolation_system = MovementInterpolationSystem::new();
        let mut direct_control_system = DirectControl::new(player_actor, 10.0, 0.4);
        let chunk_presence_system = ChunkPresenceSystem::new();
        let mut sky_light_system = SkyLightSystem::new();
        let actor_texture_loading_system =
            TextureLoadingSystem::load_data(ACTOR_TEXTURE_LIST_PATH, ACTOR_TEXTURE_PATH_PREFIX)
                .await?;

        let state_components_label_map = List::load(STATE_COMPONENTS_PATH)
            .await?
            .into_label_map(StateComponent::from_usize);

        let mut class_ac = ClassActorComponent::new(
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
        let mut animation_state_ac = AnimationStateActorComponent::new();
        let mut target_orientation_ac = TargetOrientationActorComponent::new(
            state_components_label_map.get("actor_orientation").unwrap(),
        );
        let mut target_position_ac = TargetPositionActorComponent::new(
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

        let actor_class_map = actor_class_loading_system.into_label_map();

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

        let mut interface_system = InterfaceSystemDescriptor {
            state: interface_state,
            output_thread: &output_thread,
        }
        .build();

        let mut render_system = RenderSystemDescriptor {
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

        let mut block_render_system = BlockRenderSystemDescriptor {
            render_parameters,
            block_texture_bind_group_layout,
            block_texture_bind_group,
        }
        .build(&output_thread)
        .await;

        let mut actor_render_system = ActorRenderSystemDescriptor {
            render_parameters,
            actor_texture_bind_group_layout,
            actor_texture_bind_group,
        }
        .build(&output_thread)
        .await;

        let surface_source = output_thread.get_surface_source();
        let input_source = output_thread.get_input_source();
        let mut send_state_interval = time::interval(Duration::from_millis(50));
        send_state_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut stream = stream::poll_fn(|cx| {
            send_state_interval
                .poll_tick(cx)
                .map(|_| Some(Event::SendState))
        })
        .or_ff(input_source.stream().map(Event::Input))
        .or_ff(
            surface_source
                .stream()
                // Timer::interval(Duration::from_millis(15))
                .map(|surface| Event::Process(surface))
                .rr_ff(event_rx),
        );

        let mut cursor_visible = false;
        let mut inventory_open = false;

        while let Some(event) = stream.next().await {
            match event {
                Event::Process(surface) => {
                    if inventory_open && !cursor_visible {
                        render_system.cursor_visibility(true);
                        cursor_visible = true;
                    } else if !inventory_open && cursor_visible {
                        render_system.cursor_visibility(false);
                        cursor_visible = false;
                    }

                    let now = Instant::now();
                    let elapsed = now.saturating_duration_since(last_render_time);
                    last_render_time = now;

                    // TODO: automatically scale by detecting how long it takes
                    // and how much time do we have between frames
                    let chunk_built = block_render_system.build_next_chunk(
                        &class_bc,
                        &model_bcc,
                        &builder_bmc,
                        &culling_bmc,
                        &sky_light_bc,
                        &position_ac,
                        &player_actor,
                    );

                    if !chunk_built {
                        let player_chunk = position_ac
                            .get(&player_actor)
                            .expect("player Actor must exist")
                            .chunk;

                        sky_light_system.compute_queued(
                            &class_bc,
                            &opacity_bcc,
                            &mut sky_light_bc,
                            Some(player_chunk),
                        );

                        for chunk in sky_light_system.drain_processed_chunks() {
                            block_render_system.add_chunk(chunk);
                        }
                    }

                    player_position_system.process(
                        elapsed,
                        &class_bc,
                        &collision_bcc,
                        &mut position_ac,
                        &velocity_ac,
                        snapshot,
                    );
                    chunk_presence_system.process(
                        player_chunk_view_radius,
                        &player_actor,
                        &position_ac,
                        &mut class_bc,
                        &mut status_cc,
                        |chunk| {
                            block_render_system.add_chunk(chunk);
                        },
                    );
                    direct_control_system.process(
                        elapsed,
                        &mut velocity_ac,
                        &mut orientation_ac,
                        snapshot,
                    );
                    movement_interpolation_system.process(
                        &mut target_position_ac,
                        &mut target_orientation_ac,
                        &mut position_ac,
                        &mut orientation_ac,
                        snapshot,
                    );

                    let target = player_position_system.get_target_block(
                        &position_ac,
                        &orientation_ac,
                        |chunk, block| {
                            // TODO: better targeting collision?
                            class_bc
                                .get_chunk(&chunk)
                                .map(|blocks| {
                                    let class = blocks.get(block);
                                    collision_bcc.get(class).is_some()
                                })
                                .unwrap_or(false)
                        },
                    );

                    block_render_system.build_target_highlight(target);

                    interface_system.start(render_system.output_thread().window());

                    interface_system.add_interface(|ctx| {
                        egui::Window::new("Inventory")
                            .open(&mut inventory_open)
                            .show(ctx, |ui| {
                                ui.label("Hello World!");
                            });
                    });

                    render_system.update(&position_ac, &orientation_ac);
                    actor_render_system.update(
                        player_actor,
                        &class_ac,
                        &position_ac,
                        &velocity_ac,
                        &orientation_ac,
                        &model_acc,
                        &builder_amc,
                        &mut animation_state_ac,
                    );

                    unblock!((
                        render_system,
                        block_render_system,
                        interface_system,
                        actor_render_system,
                        inventory_open
                    ) {
                        render_system.start_render(surface);

                        let mut renderers = render_system.get_renderers::<3>().into_iter();

                        block_render_system.render(renderers.next().unwrap())
                            .expect("block render");

                        actor_render_system.render(renderers.next().unwrap())
                            .expect("actor render");

                        interface_system.render(renderers.next().unwrap())
                            .expect("interface render");

                        drop(renderers);

                        render_system.finish_render();
                    });
                },
                Event::SendState => {
                    position_ac.pack_player(&mut client_state, last_client_snapshot);
                    velocity_ac.pack_player(&mut client_state, last_client_snapshot);
                    orientation_ac.pack_player(&mut client_state, last_client_snapshot);

                    let packed = ServerAccept::pack_state(
                        snapshot,
                        last_server_snapshot,
                        &mut client_state,
                        &mut packer,
                    );

                    let _ = unreliable_tx.send(packed);

                    snapshot = snapshot.next();
                },
                Event::Input(event) => {
                    match event {
                        InputEvent::DeviceEvent {
                            device_id: _,
                            event,
                        } => {
                            if !inventory_open {
                                match event {
                                    DeviceEvent::MouseMotion {
                                        delta: (horizontal, vertical),
                                    } => {
                                        direct_control_system
                                            .process_mouse(horizontal as f32, vertical as f32);
                                    },
                                    _ => {},
                                }
                            }
                        },
                        InputEvent::WindowEvent { event } => {
                            if inventory_open {
                                interface_system.window_event(&event);
                            }
                            match event {
                                WindowEvent::Resized(size) => {
                                    render_system.resize(size);
                                },
                                WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                                    return Ok(SceneSwitch::Exit);
                                },
                                WindowEvent::KeyboardInput {
                                    device_id: _,
                                    input,
                                    is_synthetic: _,
                                } => {
                                    if let Some(button) = input.virtual_keycode {
                                        if matches!(
                                            input.state,
                                            winit::event::ElementState::Pressed
                                        ) {
                                            match button {
                                                winit::event::VirtualKeyCode::Escape => break,
                                                winit::event::VirtualKeyCode::I => {
                                                    inventory_open = !inventory_open;
                                                },
                                                _ => {},
                                            }
                                        }
                                    }
                                    direct_control_system.process_keyboard(&input);
                                },
                                WindowEvent::MouseInput { state, button, .. } => {
                                    if state == ElementState::Pressed {
                                        match button {
                                            MouseButton::Left => {
                                                if let Some((chunk, block, _side)) =
                                                    player_position_system.get_target_block(
                                                        &position_ac,
                                                        &orientation_ac,
                                                        |chunk, block| {
                                                            class_bc
                                                                .get_chunk(&chunk)
                                                                .map(|blocks| {
                                                                    let class = blocks.get(block);
                                                                    collision_bcc
                                                                        .get(class)
                                                                        .is_some()
                                                                })
                                                                .unwrap_or(false)
                                                        },
                                                    )
                                                {
                                                    let _ = reliable_tx.send(packer.pack_to_vec(
                                                        &ServerAccept::AlterBlock {
                                                            chunk,
                                                            block,
                                                            block_class:
                                                                block_class_map.get("air").unwrap(),
                                                        },
                                                    ));
                                                }
                                            },
                                            MouseButton::Right => {
                                                if let Some((chunk, block, side)) =
                                                    player_position_system.get_target_block(
                                                        &position_ac,
                                                        &orientation_ac,
                                                        |chunk, block| {
                                                            class_bc
                                                                .get_chunk(&chunk)
                                                                .map(|blocks| {
                                                                    let class = blocks.get(block);
                                                                    collision_bcc
                                                                        .get(class)
                                                                        .is_some()
                                                                })
                                                                .unwrap_or(false)
                                                        },
                                                    )
                                                {
                                                    let axis = side / 2;
                                                    let direction = match side % 2 {
                                                        0 => -1,
                                                        1 => 1,
                                                        _ => panic!("incorrect side index"),
                                                    };
                                                    let mut block =
                                                        block.to_coords().map(|u| u as i32);
                                                    block[axis] += direction;
                                                    if let Some((chunk, block)) =
                                                        Block::from_chunk_offset(chunk, block)
                                                    {
                                                        let _ = reliable_tx.send(
                                                            packer.pack_to_vec(
                                                                &ServerAccept::AlterBlock {
                                                                    chunk,
                                                                    block,
                                                                    block_class: block_class_map
                                                                        .get("grass")
                                                                        .unwrap(),
                                                                },
                                                            ),
                                                        );
                                                    }
                                                }
                                            },
                                            _ => {},
                                        }
                                    }
                                },
                                _ => {},
                            }
                        },
                    }
                },
                Event::NetworkInput(result) => {
                    let message = match result {
                        Ok(m) => m,
                        Err(err) => {
                            // TODO handle properly, pass error to menu to display there
                            error!("game::run: connection error: {:?}", err);
                            return Ok(SceneSwitch::Menu {
                                parameters: MenuSceneParameters {
                                    interface_state: interface_system.into_interface_state(),
                                    output_thread: render_system.into_output(),
                                },
                            });
                        },
                    };

                    let message = match packer.unpack::<ClientAccept>(&message) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    match message {
                        ClientAccept::State {
                            snapshot: new_lss,
                            last_client_snapshot: new_lcs,
                            state,
                        } => {
                            let current_time = Instant::now();
                            class_ac.unpack_state(&state);
                            model_acc.unpack_state(&state);
                            velocity_ac.unpack_state(&state);
                            target_orientation_ac.unpack_state_convert(
                                &state,
                                |actor, previous, orientation: Orientation| {
                                    let current_value = if let Some(p) = orientation_ac.get(&actor)
                                    {
                                        *p
                                    } else {
                                        orientation_ac.insert(actor, orientation, snapshot);
                                        orientation
                                    };

                                    TargetQueue::from_previous(
                                        previous,
                                        current_value,
                                        orientation,
                                        current_time,
                                        new_lss,
                                    )
                                },
                            );
                            target_position_ac.unpack_state_convert(
                                &state,
                                |actor, previous, position: Position| {
                                    let current_value = if let Some(p) = position_ac.get(&actor) {
                                        *p
                                    } else {
                                        position_ac.insert(actor, position, snapshot);
                                        position
                                    };

                                    TargetQueue::from_previous(
                                        previous,
                                        current_value,
                                        position,
                                        current_time,
                                        new_lss,
                                    )
                                },
                            );
                            last_client_snapshot = new_lcs;
                            last_server_snapshot = new_lss;
                        },
                        ClientAccept::ChunkData(ChunkData {
                            chunk,
                            block_classes,
                        }) => {
                            class_bc.insert_chunk(chunk, block_classes);
                            status_cc.insert(chunk, ChunkStatus::Active);

                            sky_light_system.add_chunk(chunk);
                        },
                        ClientAccept::AlterBlock {
                            chunk,
                            block,
                            block_class,
                        } => {
                            if let Some(block_class_ref) =
                                class_bc.get_mut_chunk(&chunk).map(|c| c.get_mut(block))
                            {
                                *block_class_ref = block_class;

                                sky_light_system.add_chunk(chunk);
                            }
                        },
                    }
                },
            }
        }

        Ok(SceneSwitch::Menu {
            parameters: MenuSceneParameters {
                interface_state: interface_system.into_interface_state(),
                output_thread: render_system.into_output(),
            },
        })
    }
}
