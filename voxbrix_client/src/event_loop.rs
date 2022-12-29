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
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
    },
    system::{
        chunk_presence::ChunkPresenceSystem,
        controller::DirectControl,
        position::PositionSystem,
        render::RenderSystem,
    },
    window::WindowHandle,
    RenderHandle,
};
use anyhow::Result;
use async_executor::LocalExecutor;
use async_io::Timer;
use futures_lite::stream::StreamExt;
use std::{
    net::SocketAddr,
    time::{
        Duration,
        Instant,
    },
};
use voxbrix_common::{
    math::Vec3,
    messages::{
        client::{
            ClientAccept,
            InitResponse,
        },
        server::{
            InitRequest,
            ServerAccept,
        },
    },
    pack::Pack,
    unblock,
    ChunkData,
};
use voxbrix_protocol::client::Client;
use winit::{
    dpi::PhysicalSize,
    event::{
        KeyboardInput as WinitKeyboardInput,
        MouseButton as WinitMouseButton,
    },
};

pub enum EventLoop {
    Menu,
    Game { parameters: GameLoopParameters },
    Exit,
}

pub enum Event {
    Process,
    SendPosition,
    Key { input: WinitKeyboardInput },
    MouseButton { input: WinitMouseButton },
    MouseMove { horizontal: f32, vertical: f32 },
    WindowResize { new_size: PhysicalSize<u32> },
    Network { message: ClientAccept },
    DrawChunk { chunk: Chunk },
    Shutdown,
}

pub struct MenuLoop<'a> {
    pub rt: &'a LocalExecutor<'a>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
}

impl MenuLoop<'_> {
    pub async fn run(self) -> Result<EventLoop> {
        Ok(EventLoop::Game {
            parameters: GameLoopParameters {
                server: ([127, 0, 0, 1], 12000).into(),
                username: "username".to_owned(),
                password: "password".as_bytes().to_owned(),
            },
        })
    }
}

pub struct GameLoopParameters {
    pub server: SocketAddr,
    pub username: String,
    pub password: Vec<u8>,
}

pub struct GameLoop<'a> {
    pub rt: &'a LocalExecutor<'a>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
    pub parameters: GameLoopParameters,
}

impl GameLoop<'_> {
    pub async fn run(self) -> Result<()> {
        let (reliable_tx, mut reliable_rx) = local_channel::mpsc::channel::<ServerAccept>();
        let (unreliable_tx, mut unreliable_rx) = local_channel::mpsc::channel::<ServerAccept>();
        let (event_tx, event_rx) = local_channel::mpsc::channel::<Event>();

        let (tx, mut rx) = Client::bind(([0, 0, 0, 0], 0))?
            .connect(self.parameters.server)
            .await?;

        let (mut unreliable, mut reliable) = tx.split();

        let init_response = self.rt.spawn(async move {
            let mut recv_buf = Vec::new();
            let (_channel, bytes) = rx
                .recv(&mut recv_buf)
                .await
                .expect("initialization response");

            (
                rx,
                InitResponse::unpack(&bytes).expect("initialization response unpack"),
            )
        });

        reliable
            .send_reliable(
                0,
                &InitRequest {
                    username: self.parameters.username,
                    password: self.parameters.password,
                }
                .pack_to_vec(),
            )
            .await
            .expect("initialization request");

        let (mut rx, init_response) = init_response.await;

        let (player_actor, chunk_radius) = match init_response {
            InitResponse::Success {
                actor,
                player_ticket_radius,
            } => (actor, player_ticket_radius),
            InitResponse::Failure(err) => {
                panic!("{:?}", err);
            },
        };

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

        self.rt
            .spawn(async move {
                let mut send_buf = Vec::new();

                while let Some(msg) = reliable_rx.recv().await {
                    msg.pack(&mut send_buf);

                    reliable
                        .send_reliable(0, &send_buf)
                        .await
                        .expect("message sent");
                }
            })
            .detach();

        let event_tx_network = event_tx.clone();

        self.rt
            .spawn(async move {
                let mut buf = Vec::new();
                while let Ok((_channel, data)) = rx.recv(&mut buf).await {
                    let message = match ClientAccept::unpack(&data) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    if let Err(_) = event_tx_network.send(Event::Network { message }) {
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
                textures: [1, 1, 1, 1, 1, 0],
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

        let mut render_system = RenderSystem::new(
            self.render_handle,
            self.window_handle.window.inner_size(),
            &gpac,
            &oac,
        )
        .await;

        let mut stream = Timer::interval(Duration::from_millis(20))
            .map(|_| Event::Process)
            .or(self.window_handle.event_rx.stream())
            .or(Timer::interval(Duration::from_millis(50)).map(|_| Event::SendPosition))
            .or(event_rx);

        while let Some(event) = stream.next().await {
            match event {
                Event::Process => {
                    let elapsed = last_render_time.elapsed();
                    last_render_time = Instant::now();

                    // TODO consider what should really be unblocked?
                    // let time_test = Instant::now();
                    position_system.process(elapsed, &cbc, &mut gpac, &vac);
                    chunk_presence_system.process(
                        chunk_radius,
                        &player_actor,
                        &gpac,
                        &mut cbc,
                        &mut scc,
                        &event_tx,
                    );
                    direct_control_system.process(elapsed, &mut vac, &mut oac);
                    render_system.update(&gpac, &oac);

                    unblock!((render_system) {
                        render_system.render()
                            .expect("render");
                    });
                    // log::error!("Elapsed: {:?}", time_test.elapsed());
                },
                Event::SendPosition => {
                    let player_position = gpac.get(&player_actor).unwrap();
                    unreliable_tx.send(ServerAccept::PlayerPosition {
                        position: player_position.clone(),
                    });
                },
                Event::Key { input } => {
                    direct_control_system.process_keyboard(&input);
                },
                Event::MouseButton { input } => {
                    match input {
                        WinitMouseButton::Left => {
                            let position = gpac.get(&player_actor).unwrap();
                            let orientation = oac.get(&player_actor).unwrap();

                            PositionSystem::get_target_block(
                                position,
                                orientation,
                                |chunk, block| {
                                    cbc.get_chunk(&chunk)
                                        .map(|blocks| blocks.get(block).unwrap() == &BlockClass(1))
                                        .unwrap_or(false)
                                },
                            )
                            .and_then(|(chunk, block, _side)| {
                                /*
                                let block_class =
                                    cbc.get_mut_chunk(&chunk).and_then(|c| c.get_mut(block))?;

                                *block_class = BlockClass(0);

                                sender_tx.send(ServerAccept::AlterBlock {
                                    chunk,
                                    block,
                                    block_class: *block_class,
                                });

                                let _ = event_tx.send(Event::DrawChunk { chunk });
                                */

                                reliable_tx.send(ServerAccept::AlterBlock {
                                    chunk,
                                    block,
                                    block_class: BlockClass(0),
                                });

                                Some(())
                            });
                        },
                        WinitMouseButton::Right => {
                            let position = gpac.get(&player_actor).unwrap();
                            let orientation = oac.get(&player_actor).unwrap();

                            PositionSystem::get_target_block(
                                position,
                                orientation,
                                |chunk, block| {
                                    cbc.get_chunk(&chunk)
                                        .map(|blocks| blocks.get(block).unwrap() == &BlockClass(1))
                                        .unwrap_or(false)
                                },
                            )
                            .and_then(|(chunk, block, side)| {
                                /*
                                 * 
                                let axis = side / 2;
                                let direction = match side % 2 {
                                    0 => -1,
                                    1 => 1,
                                    _ => panic!("incorrect side index"),
                                };
                                let mut block = block.to_coords().map(|u| u as i32);
                                block[axis] += direction;
                                let (chunk, block) = Block::from_chunk_offset(chunk, block);
                                let block_class =
                                    cbc.get_mut_chunk(&chunk).and_then(|c| c.get_mut(block))?;

                                *block_class = BlockClass(1);

                                sender_tx.send(ServerAccept::AlterBlock {
                                    chunk,
                                    block,
                                    block_class: *block_class,
                                });

                                let _ = event_tx.send(Event::DrawChunk { chunk });
                                */
                                let axis = side / 2;
                                let direction = match side % 2 {
                                    0 => -1,
                                    1 => 1,
                                    _ => panic!("incorrect side index"),
                                };
                                let mut block = block.to_coords().map(|u| u as i32);
                                block[axis] += direction;
                                let (chunk, block) = Block::from_chunk_offset(chunk, block);

                                reliable_tx.send(ServerAccept::AlterBlock {
                                    chunk,
                                    block,
                                    block_class: BlockClass(1),
                                });

                                Some(())
                            });
                        },
                        _ => {},
                    }
                },
                Event::MouseMove {
                    horizontal,
                    vertical,
                } => {
                    direct_control_system.process_mouse(horizontal, vertical);
                },
                Event::WindowResize { new_size } => {
                    render_system.resize(new_size);
                },
                Event::Network { message } => {
                    match message {
                        ClientAccept::ChunkData(ChunkData {
                            chunk,
                            block_classes,
                        }) => {
                            cbc.insert_chunk(chunk, block_classes);
                            scc.insert(chunk, ChunkStatus::Active);
                            let _ = event_tx.send(Event::DrawChunk { chunk });
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

                                let _ = event_tx.send(Event::DrawChunk { chunk });
                            }
                        },
                    }
                },
                Event::DrawChunk { chunk } => {
                    unblock!((render_system, cbc, mbcc) {
                        render_system.build_chunk(&chunk, &cbc, &mbcc);
                    });
                },
                Event::Shutdown => break,
            }
        }

        Ok(())
    }
}

pub struct MainLoop<'a> {
    pub rt: &'a LocalExecutor<'a>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
}

impl MainLoop<'_> {
    pub async fn run(self) -> Result<()> {
        let mut next_loop = Some(EventLoop::Menu);

        loop {
            match next_loop.take().unwrap_or_else(|| EventLoop::Exit) {
                EventLoop::Menu => {
                    next_loop = Some(
                        MenuLoop {
                            rt: self.rt,
                            window_handle: self.window_handle,
                            render_handle: self.render_handle,
                        }
                        .run()
                        .await?,
                    );
                },
                EventLoop::Game { parameters } => {
                    GameLoop {
                        rt: self.rt,
                        window_handle: self.window_handle,
                        render_handle: self.render_handle,
                        parameters,
                    }
                    .run()
                    .await?;
                },
                EventLoop::Exit => return Ok(()),
            }
        }
    }
}
