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
            Target,
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
    scene::SceneSwitch,
    system::{
        actor_render::ActorRenderSystemDescriptor,
        block_render::BlockRenderSystemDescriptor,
        chunk_presence::ChunkPresenceSystem,
        controller::DirectControl,
        model_loading::ModelLoadingSystem,
        movement_interpolation::MovementInterpolationSystem,
        player_position::PlayerPositionSystem,
        render::{
            camera::CameraParameters,
            output_thread::OutputBundle,
            RenderSystemDescriptor,
        },
        texture_loading::TextureLoadingSystem,
    },
    window::{
        InputEvent,
        WindowHandle,
    },
    RenderHandle,
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
    DrawChunk(Chunk),
    ComputeLight,
}

pub struct GameSceneParameters {
    pub connection: (Sender, Receiver),
    pub player_actor: Actor,
    pub player_chunk_view_radius: i32,
}

pub struct GameScene {
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
    pub parameters: GameSceneParameters,
}

impl GameScene {
    pub async fn run(self) -> Result<SceneSwitch> {
        let (reliable_tx, mut reliable_rx) = local_channel::mpsc::channel::<Vec<u8>>();
        let (unreliable_tx, mut unreliable_rx) = local_channel::mpsc::channel::<Vec<u8>>();
        let (event_tx, event_rx) = local_channel::mpsc::channel::<Event>();

        let mut client_state = StatePacker::new();

        let mut snapshot = Snapshot(1);
        // Last client's snapshot received by the server
        let mut last_client_snapshot = Snapshot(0);
        let mut last_server_snapshot = Snapshot(0);

        let mut packer = Packer::new();

        let GameSceneParameters {
            connection,
            player_actor,
            player_chunk_view_radius,
        } = self.parameters;

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
                    position: [0, 0, 0].into(),
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

        self.window_handle
            .window
            .set_cursor_grab(winit::window::CursorGrabMode::Confined)
            .or_else(|_| {
                self.window_handle
                    .window
                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
            })?;
        self.window_handle.window.set_cursor_visible(false);

        let (block_texture_bind_group_layout, block_texture_bind_group) =
            block_texture_loading_system
                .prepare_buffer(&self.render_handle.device, &self.render_handle.queue);

        let (actor_texture_bind_group_layout, actor_texture_bind_group) =
            actor_texture_loading_system
                .prepare_buffer(&self.render_handle.device, &self.render_handle.queue);

        let surface_size = self.window_handle.window.inner_size();

        let mut render_system = RenderSystemDescriptor {
            render_handle: self.render_handle,
            window_handle: self.window_handle,
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
        }
        .build()
        .await;

        let render_parameters = render_system.get_render_parameters();

        let mut block_render_system = BlockRenderSystemDescriptor {
            render_handle: self.render_handle,
            render_parameters,
            block_texture_bind_group_layout,
            block_texture_bind_group,
        }
        .build()
        .await;

        let mut actor_render_system = ActorRenderSystemDescriptor {
            render_handle: self.render_handle,
            render_parameters,
            actor_texture_bind_group_layout,
            actor_texture_bind_group,
        }
        .build()
        .await;

        let surface_stream = render_system.get_surface_stream();
        let mut send_state_interval = time::interval(Duration::from_millis(50));
        send_state_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut stream = stream::poll_fn(|cx| {
            send_state_interval
                .poll_tick(cx)
                .map(|_| Some(Event::SendState))
        })
        .or_ff(self.window_handle.event_rx.stream().map(Event::Input))
        .or_ff(
            surface_stream
                .stream()
                // Timer::interval(Duration::from_millis(15))
                .map(|surface| Event::Process(surface))
                .rr_ff(event_rx),
        );

        while let Some(event) = stream.next().await {
            match event {
                Event::Process(surface) => {
                    let now = Instant::now();
                    let elapsed = now.saturating_duration_since(last_render_time);
                    last_render_time = now;

                    // TODO consider what should really be unblocked?
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
                        &event_tx,
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

                    unblock!((render_system, block_render_system, actor_render_system) {
                        render_system.start_render(surface);

                        let mut renderers = render_system.get_renderers::<2>().into_iter();

                        block_render_system.render(renderers.next().unwrap())
                            .expect("block render");

                        actor_render_system.render(renderers.next().unwrap())
                            .expect("actor render");

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
                            match event {
                                DeviceEvent::MouseMotion {
                                    delta: (horizontal, vertical),
                                } => {
                                    direct_control_system
                                        .process_mouse(horizontal as f32, vertical as f32);
                                },
                                _ => {},
                            }
                        },
                        InputEvent::WindowEvent { event } => {
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
                                    if let Some(winit::event::VirtualKeyCode::Escape) =
                                        input.virtual_keycode
                                    {
                                        break;
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
                                                    let (chunk, block) =
                                                        Block::from_chunk_offset(chunk, block);

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
                            return Ok(SceneSwitch::Menu);
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
                            let reach_time = Instant::now();
                            class_ac.unpack_state(&state);
                            model_acc.unpack_state(&state);
                            velocity_ac.unpack_state(&state);
                            target_orientation_ac.unpack_state_convert(
                                &state,
                                |actor, previous, orientation: Orientation| {
                                    if orientation_ac.get(&actor).is_none() {
                                        orientation_ac.insert(actor, orientation, snapshot);
                                    }

                                    let mut target_queue = previous.unwrap_or(TargetQueue {
                                        starting: orientation_ac
                                            .get(&actor)
                                            .cloned()
                                            .unwrap_or(orientation),
                                        target_queue: arrayvec::ArrayVec::new(),
                                    });

                                    let last = target_queue.target_queue.last().copied();

                                    if last.is_none()
                                        || last.is_some() && last.unwrap().server_snapshot < new_lss
                                    {
                                        let new = Target {
                                            server_snapshot: new_lss,
                                            value: orientation,
                                            reach_time,
                                        };

                                        if target_queue.target_queue.is_full() {
                                            *target_queue.target_queue.last_mut().unwrap() = new;
                                        } else {
                                            target_queue.target_queue.push(new);
                                        }
                                    }

                                    target_queue
                                },
                            );
                            target_position_ac.unpack_state_convert(
                                &state,
                                |actor, previous, position: Position| {
                                    if position_ac.get(&actor).is_none() {
                                        position_ac.insert(actor, position, snapshot);
                                    }

                                    let mut target_queue = previous.unwrap_or(TargetQueue {
                                        starting: position_ac
                                            .get(&actor)
                                            .cloned()
                                            .unwrap_or(position),
                                        target_queue: arrayvec::ArrayVec::new(),
                                    });

                                    let last = target_queue.target_queue.last().copied();

                                    if last.is_none()
                                        || last.is_some() && last.unwrap().server_snapshot < new_lss
                                    {
                                        let new = Target {
                                            server_snapshot: new_lss,
                                            value: position,
                                            reach_time,
                                        };

                                        if target_queue.target_queue.is_full() {
                                            *target_queue.target_queue.last_mut().unwrap() = new;
                                        } else {
                                            target_queue.target_queue.push(new);
                                        }
                                    }

                                    target_queue
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
                            let _ = event_tx.send(Event::ComputeLight);
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
                                let _ = event_tx.send(Event::ComputeLight);
                            }
                        },
                    }
                },
                Event::DrawChunk(chunk) => {
                    // TODO: use separate *set* as a queue and take up to num_cpus each time the event
                    // comes
                    unblock!((block_render_system, class_bc, model_bcc, builder_bmc, culling_bmc, sky_light_bc) {
                        block_render_system.build_chunk(&chunk, &class_bc, &model_bcc, &builder_bmc, &culling_bmc, &sky_light_bc);
                    });
                },
                Event::ComputeLight => {
                    sky_light_system.compute_queued(&class_bc, &opacity_bcc, &mut sky_light_bc);

                    for chunk in sky_light_system.drain_processed_chunks() {
                        let _ = event_tx.send(Event::DrawChunk(chunk));
                    }

                    if sky_light_system.has_chunks_in_queue() {
                        let _ = event_tx.send(Event::ComputeLight);
                    }
                },
            }
        }

        Ok(SceneSwitch::Menu)
    }
}
