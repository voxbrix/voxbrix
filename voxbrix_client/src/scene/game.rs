use crate::{
    component::{
        actor::{
            orientation::{
                Orientation,
                OrientationActorComponent,
            },
            position::{
                GlobalPosition,
                GlobalPositionActorComponent,
            },
            velocity::{
                Velocity,
                VelocityActorComponent,
            },
        },
        block::class::ClassBlockComponent,
        block_class::model::{
            Cube,
            Model,
            ModelBlockClassComponent,
        },
        chunk::status::{
            ChunkStatus,
            StatusChunkComponent,
        },
    },
    entity::{
        actor::Actor,
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
    },
    scene::SceneSwitch,
    system::{
        chunk_presence::ChunkPresenceSystem,
        controller::DirectControl,
        position::PositionSystem,
        render::RenderSystem,
    },
    window::{
        InputEvent,
        WindowEvent,
        WindowHandle,
    },
    RenderHandle,
};
use anyhow::Result;
use async_executor::LocalExecutor;
use async_io::Timer;
use futures_lite::{
    stream::StreamExt,
    future::FutureExt,
};
use std::time::{
    Duration,
    Instant,
};
use voxbrix_common::{
    math::Vec3,
    messages::{
        client::ClientAccept,
        server::ServerAccept,
    },
    pack::PackZip,
    stream::StreamExt as _,
    unblock,
    ChunkData,
};
use voxbrix_protocol::client::{
    Receiver,
    Error as ClientError,
    Sender,
};
use winit::event::{
    DeviceEvent,
    ElementState,
    MouseButton,
};
use crate::CONNECTION_TIMEOUT;
use std::io::ErrorKind as StdIoErrorKind;
use log::error;

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
    pub rt: &'a LocalExecutor<'a>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
    pub parameters: GameSceneParameters,
}

impl GameScene<'_> {
    pub async fn run(self) -> Result<SceneSwitch> {
        let (reliable_tx, mut reliable_rx) = local_channel::mpsc::channel::<ServerAccept>();
        let (unreliable_tx, mut unreliable_rx) = local_channel::mpsc::channel::<ServerAccept>();
        let (event_tx, event_rx) = local_channel::mpsc::channel::<Event>();

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

        self.rt
            .spawn(async move {
                let mut buf = Vec::new();
                loop {
                    let data = match rx.recv(&mut buf).await {
                        Ok((_channel, data)) => data,
                        Err(err) => {
                            let _ = event_tx_network.send(Event::NetworkInput(Err(err)));
                            break;
                        },
                    };
                    
                    let message = match ClientAccept::unpack(&data) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    if let Err(_) = event_tx_network.send(Event::NetworkInput(Ok(message))) {
                        break;
                    };
                }
            })
            .detach();

        let mut scc = StatusChunkComponent::new();

        let mut cbc = ClassBlockComponent::new();
        let mut mbcc = ModelBlockClassComponent::new();

        mbcc.set(
            BlockClass(1),
            Model::Cube(Cube {
                textures: [2, 2, 2, 2, 2, 1],
            }),
        );

        let mut last_render_time = Instant::now();

        let mut position_system = PositionSystem::new();
        let mut direct_control_system = DirectControl::new(player_actor, 10.0, 0.4);
        let chunk_presence_system = ChunkPresenceSystem::new();

        let mut gpac = GlobalPositionActorComponent::new();
        let mut vac = VelocityActorComponent::new();
        let mut oac = OrientationActorComponent::new();

        gpac.insert(
            player_actor,
            GlobalPosition {
                chunk: Chunk {
                    position: [0, 0, 0].into(),
                    dimension: 0,
                },
                offset: Vec3::new(0.0, 0.0, 4.0),
            },
        );
        vac.insert(
            player_actor,
            Velocity {
                vector: Vec3::new(0.0, 0.0, 0.0),
            },
        );
        oac.insert(player_actor, Orientation::from_yaw_pitch(0.0, 0.0));

        self.window_handle
            .window
            .set_cursor_grab(winit::window::CursorGrabMode::Confined)
            .or_else(|_| {
                self.window_handle
                    .window
                    .set_cursor_grab(winit::window::CursorGrabMode::Locked)
            })?;
        self.window_handle.window.set_cursor_visible(false);

        let mut render_system = RenderSystem::new(
            self.render_handle,
            self.window_handle.window.inner_size(),
            player_actor,
            &gpac,
            &oac,
        )
        .await;

        let mut stream = Timer::interval(Duration::from_millis(20))
            .map(|_| Event::Process)
            .or_ff(self.window_handle.event_rx.stream().map(Event::Input))
            .or_ff(Timer::interval(Duration::from_millis(50)).map(|_| Event::SendPosition))
            .or_ff(event_rx);

        while let Some(event) = stream.next().await {
            match event {
                Event::Process => {
                    let elapsed = last_render_time.elapsed();
                    last_render_time = Instant::now();

                    // TODO consider what should really be unblocked?
                    // let time_test = Instant::now();
                    position_system.process(elapsed, &cbc, &mut gpac, &vac);
                    chunk_presence_system.process(
                        player_ticket_radius,
                        &player_actor,
                        &gpac,
                        &mut cbc,
                        &mut scc,
                        &event_tx,
                    );
                    direct_control_system.process(elapsed, &mut vac, &mut oac);
                    render_system.update(&gpac, &oac);

                    let position = gpac.get(&player_actor).unwrap();
                    let orientation = oac.get(&player_actor).unwrap();

                    let target =
                        PositionSystem::get_target_block(position, orientation, |chunk, block| {
                            cbc.get_chunk(&chunk)
                                .map(|blocks| blocks.get(block).unwrap() == &BlockClass(1))
                                .unwrap_or(false)
                        });

                    render_system.build_target_highlight(target);

                    unblock!((render_system) {
                        render_system.render()
                            .expect("render");
                    });
                    // log::error!("Elapsed: {:?}", time_test.elapsed());
                },
                Event::SendPosition => {
                    let player_position = gpac.get(&player_actor).unwrap();
                    let _ = unreliable_tx.send(ServerAccept::PlayerPosition {
                        position: player_position.clone(),
                    });
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
                                    if let Some(winit::event::VirtualKeyCode::Escape) = input.virtual_keycode {
                                        break;
                                    }
                                    direct_control_system.process_keyboard(&input);
                                },
                                WindowEvent::MouseInput {
                                    device_id: _,
                                    state,
                                    button,
                                } => {
                                    if state == ElementState::Pressed {
                                        match button {
                                            MouseButton::Left => {
                                                let position = gpac.get(&player_actor).unwrap();
                                                let orientation = oac.get(&player_actor).unwrap();

                                                PositionSystem::get_target_block(
                                                    position,
                                                    orientation,
                                                    |chunk, block| {
                                                        cbc.get_chunk(&chunk)
                                                            .map(|blocks| {
                                                                blocks.get(block).unwrap()
                                                                    == &BlockClass(1)
                                                            })
                                                            .unwrap_or(false)
                                                    },
                                                )
                                                .and_then(|(chunk, block, _side)| {
                                                    let _ = reliable_tx.send(ServerAccept::AlterBlock {
                                                        chunk,
                                                        block,
                                                        block_class: BlockClass(0),
                                                    });

                                                    Some(())
                                                });
                                            },
                                            MouseButton::Right => {
                                                let position = gpac.get(&player_actor).unwrap();
                                                let orientation = oac.get(&player_actor).unwrap();

                                                PositionSystem::get_target_block(
                                                    position,
                                                    orientation,
                                                    |chunk, block| {
                                                        cbc.get_chunk(&chunk)
                                                            .map(|blocks| {
                                                                blocks.get(block).unwrap()
                                                                    == &BlockClass(1)
                                                            })
                                                            .unwrap_or(false)
                                                    },
                                                )
                                                .and_then(|(chunk, block, side)| {
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

                                                    let _ = reliable_tx.send(ServerAccept::AlterBlock {
                                                        chunk,
                                                        block,
                                                        block_class: BlockClass(1),
                                                    });

                                                    Some(())
                                                });
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
                            //TODO handle properly, pass error to menu to display there
                            error!("game::run: connection error: {:?}", err);
                            return Ok(SceneSwitch::Menu);
                        },
                    };
                    
                    match message {
                        ClientAccept::ChunkData(ChunkData {
                            chunk,
                            block_classes,
                        }) => {
                            cbc.insert_chunk(chunk, block_classes);
                            scc.insert(chunk, ChunkStatus::Active);
                            let _ = event_tx.send(Event::DrawChunk(chunk));
                        },
                        ClientAccept::AlterBlock {
                            chunk,
                            block,
                            block_class,
                        } => {
                            if let Some(block_class_ref) =
                                cbc.get_mut_chunk(&chunk).and_then(|c| c.get_mut(block))
                            {
                                *block_class_ref = block_class;

                                let _ = event_tx.send(Event::DrawChunk(chunk));
                            }
                        },
                    }
                },
                Event::DrawChunk(chunk) => {
                    // TODO: use separate *set* as a queue and take up to num_cpus each time the event
                    // comes
                    unblock!((render_system, cbc, mbcc) {
                        render_system.build_chunk(&chunk, &cbc, &mbcc);
                    });
                },
            }
        }

        Ok(SceneSwitch::Menu)
    }
}
