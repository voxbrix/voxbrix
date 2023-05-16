use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_model::body_part::BodyPartActorModelComponent,
        block_class::{
            culling::{
                Culling,
                CullingBlockClassComponent,
            },
            model::{
                Cube,
                Model,
                ModelBlockClassComponent,
                ModelDescriptor,
            },
        },
    },
    scene::SceneSwitch,
    system::{
        actor_model_loading::ActorModelLoadingSystem,
        actor_render::ActorRenderSystemDescriptor,
        block_render::BlockRenderSystemDescriptor,
        chunk_presence::ChunkPresenceSystem,
        controller::DirectControl,
        position::PositionSystem,
        render::{
            camera::CameraParameters,
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
use async_executor::LocalExecutor;
use async_io::Timer;
use futures_lite::{
    future::FutureExt,
    stream::StreamExt,
};
use log::error;
use std::{
    io::ErrorKind as StdIoErrorKind,
    rc::Rc,
    time::{
        Duration,
        Instant,
    },
};
use voxbrix_common::{
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
        block::Block,
        chunk::Chunk,
    },
    math::Vec3,
    messages::{
        client::{
            ActorStatus,
            ClientAccept,
        },
        server::ServerAccept,
    },
    pack::PackZip,
    stream::StreamExt as _,
    system::{
        block_class_loading::BlockClassLoadingSystem,
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
    Process,
    SendPosition,
    Input(InputEvent),
    NetworkInput(Result<ClientAccept, ClientError>),
    DrawChunk(Chunk),
}

pub struct GameSceneParameters {
    pub connection: (Sender, Receiver),
    pub player_actor: Actor,
    pub player_ticket_radius: i32,
}

pub struct GameScene<'a> {
    pub rt: Rc<LocalExecutor<'a>>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
    pub parameters: GameSceneParameters,
}

impl GameScene<'_> {
    pub async fn run(self) -> Result<SceneSwitch> {
        let (reliable_tx, mut reliable_rx) = local_channel::mpsc::channel::<ServerAccept>();
        let (unreliable_tx, mut unreliable_rx) = local_channel::mpsc::channel::<ServerAccept>();
        let (event_tx, event_rx) = local_channel::mpsc::channel::<Event>();

        let mut status_timestamp = Duration::ZERO;

        let GameSceneParameters {
            connection,
            player_actor,
            player_ticket_radius,
        } = self.parameters;

        let (tx, mut rx) = connection;

        let (mut unreliable, mut reliable) = tx.split();

        self.rt
            .spawn(async move {
                let mut send_buf = Vec::new();

                while let Some(msg) = unreliable_rx.recv().await {
                    msg.pack(&mut send_buf);

                    unreliable
                        .send_unreliable(0, &send_buf)
                        .await
                        .expect("message sent");
                }
            })
            .detach();

        let event_tx_network = event_tx.clone();

        self.rt
            .spawn(async move {
                let mut send_buf = Vec::new();

                while let Some(msg) = reliable_rx.recv().await {
                    msg.pack(&mut send_buf);

                    if let Err(err) = reliable
                        .send_reliable(0, &send_buf)
                        .or(async {
                            Timer::after(CONNECTION_TIMEOUT).await;
                            Err(ClientError::Io(StdIoErrorKind::TimedOut.into()))
                        })
                        .await
                    {
                        let _ = event_tx_network.send(Event::NetworkInput(Err(err)));
                        break;
                    }
                }
            })
            .detach();

        let event_tx_network = event_tx.clone();

        // Should be dropped when the loop ends
        let _recv_task = self.rt.spawn(async move {
            loop {
                let data = match rx.recv().await {
                    Ok((_channel, data)) => data,
                    Err(err) => {
                        let _ = event_tx_network.send(Event::NetworkInput(Err(err)));
                        break;
                    },
                };

                let message = match ClientAccept::unpack(data) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if event_tx_network
                    .send(Event::NetworkInput(Ok(message)))
                    .is_err()
                {
                    break;
                };
            }
        });

        let block_class_loading_system = BlockClassLoadingSystem::load_data().await?;
        let block_texture_loading_system = TextureLoadingSystem::load_data("blocks").await?;

        let mut status_cc = StatusChunkComponent::new();

        let mut class_bc = ClassBlockComponent::new();
        let mut sky_light_bc = SkyLightBlockComponent::new();

        let mut model_bcc = ModelBlockClassComponent::new();
        let mut culling_bcc = CullingBlockClassComponent::new();
        let mut collision_bcc = CollisionBlockClassComponent::new();
        let mut opacity_bcc = OpacityBlockClassComponent::new();

        block_class_loading_system.load_component(
            "model",
            &mut model_bcc,
            |desc: ModelDescriptor| {
                match desc {
                    ModelDescriptor::Cube {
                        textures: textures_desc,
                    } => {
                        let mut textures = [0; 6];

                        for (i, texture) in textures.iter_mut().enumerate() {
                            let texture_name = textures_desc[i].as_str();
                            *texture = block_texture_loading_system
                                .label_map
                                .get(texture_name)
                                .ok_or_else(|| {
                                    anyhow::Error::msg(format!(
                                        "texture \"{}\" not found",
                                        texture_name
                                    ))
                                })?;
                        }

                        Ok(Model::Cube(Cube { textures }))
                    },
                }
            },
        )?;

        block_class_loading_system.load_component(
            "culling",
            &mut culling_bcc,
            |desc: Culling| Ok(desc),
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

        let mut position_system = PositionSystem::new();
        let mut direct_control_system = DirectControl::new(player_actor, 10.0, 0.4);
        let chunk_presence_system = ChunkPresenceSystem::new();
        let sky_light_system = SkyLightSystem::new();
        let actor_texture_loading_system = TextureLoadingSystem::load_data("actors").await?;

        let mut class_ac = ClassActorComponent::new();
        let mut position_ac = PositionActorComponent::new();
        let mut velocity_ac = VelocityActorComponent::new();
        let mut orientation_ac = OrientationActorComponent::new();

        let mut body_part_amc = BodyPartActorModelComponent::new();
        let ActorModelLoadingSystem {
            model_label_map,
            body_part_label_map,
        } = ActorModelLoadingSystem::load_data(
            &actor_texture_loading_system.label_map,
            &mut body_part_amc,
        )
        .await
        .expect("actor model loading");

        position_ac.insert(
            player_actor,
            Position {
                chunk: Chunk {
                    position: [0, 0, 0].into(),
                    dimension: 0,
                },
                offset: Vec3::new(0.0, 0.0, 4.0),
            },
        );
        velocity_ac.insert(
            player_actor,
            Velocity {
                vector: Vec3::new(0.0, 0.0, 0.0),
            },
        );
        orientation_ac.insert(player_actor, Orientation::from_yaw_pitch(0.0, 0.0));

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

        let render_ready = render_system.get_readiness_stream();

        let mut stream = Timer::interval(Duration::from_millis(50))
            .map(|_| Event::SendPosition)
            .or_ff(self.window_handle.event_rx.stream().map(Event::Input))
            .or_ff(
                render_ready
                    .stream()
                    // Timer::interval(Duration::from_millis(15))
                    .map(|_| Event::Process)
                    .rr_ff(event_rx),
            );

        while let Some(event) = stream.next().await {
            match event {
                Event::Process => {
                    let elapsed = last_render_time.elapsed();
                    last_render_time = Instant::now();

                    // TODO consider what should really be unblocked?
                    position_system.process(
                        elapsed,
                        &class_bc,
                        &collision_bcc,
                        &mut position_ac,
                        &velocity_ac,
                    );
                    chunk_presence_system.process(
                        player_ticket_radius,
                        &player_actor,
                        &position_ac,
                        &mut class_bc,
                        &mut status_cc,
                        &event_tx,
                    );
                    direct_control_system.process(elapsed, &mut velocity_ac, &mut orientation_ac);

                    let position = position_ac.get(&player_actor).unwrap();
                    let orientation = orientation_ac.get(&player_actor).unwrap();

                    let target =
                        PositionSystem::get_target_block(position, orientation, |chunk, block| {
                            // TODO: better targeting collision?
                            class_bc
                                .get_chunk(&chunk)
                                .map(|blocks| {
                                    let class = blocks.get(block);
                                    collision_bcc.get(*class).is_some()
                                })
                                .unwrap_or(false)
                        });

                    block_render_system.build_target_highlight(target);

                    render_system.update(&position_ac, &orientation_ac);
                    actor_render_system.update(
                        player_actor,
                        &class_ac,
                        &position_ac,
                        &body_part_amc,
                    );

                    unblock!((render_system, block_render_system, actor_render_system) {
                        render_system.start_render()
                            .expect("start render process");

                        let mut renderers = render_system.get_renderers::<2>().into_iter();

                        block_render_system.render(renderers.next().unwrap())
                            .expect("block render");

                        actor_render_system.render(renderers.next().unwrap())
                            .expect("actor render");

                        drop(renderers);

                        render_system.finish_render();
                    });
                },
                Event::SendPosition => {
                    let position = position_ac.get(&player_actor).unwrap().clone();
                    let velocity = velocity_ac.get(&player_actor).unwrap().clone();
                    let _ = unreliable_tx.send(ServerAccept::PlayerMovement { position, velocity });
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
                                                let position =
                                                    position_ac.get(&player_actor).unwrap();
                                                let orientation =
                                                    orientation_ac.get(&player_actor).unwrap();

                                                if let Some((chunk, block, _side)) =
                                                    PositionSystem::get_target_block(
                                                        position,
                                                        orientation,
                                                        |chunk, block| {
                                                            class_bc
                                                                .get_chunk(&chunk)
                                                                .map(|blocks| {
                                                                    let class = blocks.get(block);
                                                                    collision_bcc
                                                                        .get(*class)
                                                                        .is_some()
                                                                })
                                                                .unwrap_or(false)
                                                        },
                                                    )
                                                {
                                                    let _ = reliable_tx.send(
                                                        ServerAccept::AlterBlock {
                                                            chunk,
                                                            block,
                                                            block_class: block_class_map
                                                                .get("air")
                                                                .unwrap(),
                                                        },
                                                    );
                                                }
                                            },
                                            MouseButton::Right => {
                                                let position =
                                                    position_ac.get(&player_actor).unwrap();
                                                let orientation =
                                                    orientation_ac.get(&player_actor).unwrap();

                                                if let Some((chunk, block, side)) =
                                                    PositionSystem::get_target_block(
                                                        position,
                                                        orientation,
                                                        |chunk, block| {
                                                            class_bc
                                                                .get_chunk(&chunk)
                                                                .map(|blocks| {
                                                                    let class = blocks.get(block);
                                                                    collision_bcc
                                                                        .get(*class)
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
                                                        ServerAccept::AlterBlock {
                                                            chunk,
                                                            block,
                                                            block_class: block_class_map
                                                                .get("grass")
                                                                .unwrap(),
                                                        },
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

                    match message {
                        ClientAccept::ChunkData(ChunkData {
                            chunk,
                            block_classes,
                        }) => {
                            class_bc.insert_chunk(chunk, block_classes);
                            status_cc.insert(chunk, ChunkStatus::Active);

                            let chunks_to_redraw = sky_light_system.calc_chunk_finalize(
                                chunk,
                                &class_bc,
                                &opacity_bcc,
                                &mut sky_light_bc,
                            );

                            for chunk in chunks_to_redraw {
                                let _ = event_tx.send(Event::DrawChunk(chunk));
                            }
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

                                let chunks_to_redraw = sky_light_system.calc_chunk_finalize(
                                    chunk,
                                    &class_bc,
                                    &opacity_bcc,
                                    &mut sky_light_bc,
                                );

                                for chunk in chunks_to_redraw {
                                    let _ = event_tx.send(Event::DrawChunk(chunk));
                                }
                            }
                        },
                        ClientAccept::ActorStatus { timestamp, status } => {
                            if timestamp > status_timestamp {
                                for ActorStatus {
                                    actor,
                                    class,
                                    position,
                                    velocity,
                                } in status
                                {
                                    class_ac.insert(actor, class);
                                    if actor != player_actor {
                                        position_ac.insert(actor, position);
                                        velocity_ac.insert(actor, velocity);
                                    }
                                }

                                status_timestamp = timestamp;
                            }
                        },
                    }
                },
                Event::DrawChunk(chunk) => {
                    // TODO: use separate *set* as a queue and take up to num_cpus each time the event
                    // comes
                    unblock!((block_render_system, class_bc, model_bcc, culling_bcc, sky_light_bc) {
                        block_render_system.build_chunk(&chunk, &class_bc, &model_bcc, &culling_bcc, &sky_light_bc);
                    });
                },
            }
        }

        Ok(SceneSwitch::Menu)
    }
}
