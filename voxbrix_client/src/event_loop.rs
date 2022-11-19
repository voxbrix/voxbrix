use crate::{
    component::{
        actor::{
            facing::{
                Facing,
                FacingActorComponent,
            },
            position::{
                Position,
                PositionActorComponent,
            },
            velocity::{
                Velocity,
                VelocityActorComponent,
            },
        },
        block::{
            class::ClassBlockComponent,
            Blocks,
        },
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
        block_class::BlockClass,
        chunk::Chunk,
    },
    linear_algebra::Vec3,
    system::{
        controller::DirectControl,
        position::PositionSystem,
        render::RenderSystem,
    },
    window::WindowHandle,
};
use anyhow::Result;
use async_executor::LocalExecutor;
use async_io::Timer;
use futures_lite::stream::StreamExt;
use std::{
    collections::BTreeSet,
    time::{
        Duration,
        Instant,
    },
};
use voxbrix_messages::{
    client::ClientAccept,
    server::ServerAccept,
    Chunk as ChunkRequest,
    Pack,
};
use voxbrix_protocol::client::Client;
use winit::{
    dpi::PhysicalSize,
    event::KeyboardInput as WinitKeyboardInput,
};

pub enum Event {
    Process,
    // Render,
    Key { input: WinitKeyboardInput },
    MouseMove { horizontal: f32, vertical: f32 },
    WindowResize { new_size: PhysicalSize<u32> },
    Network { message: ClientAccept },
    NearbyChunksUpdate,
    DrawChunk { chunk: Chunk },
    Shutdown,
}

pub struct EventLoop<'a> {
    pub rt: &'a LocalExecutor<'a>,
    pub window: WindowHandle,
}

impl EventLoop<'_> {
    pub async fn run(self) -> Result<()> {
        let WindowHandle {
            instance,
            surface,
            size: surface_size,
            event_rx: window_event_rx,
        } = self.window;

        let (sender_tx, mut sender_rx) = local_channel::mpsc::channel::<ServerAccept>();
        let (event_tx, event_rx) = local_channel::mpsc::channel::<Event>();

        let mut send_buf = Vec::new();

        let (mut tx, mut rx) = Client::bind(([127, 0, 0, 1], 12001))?
            .connect(([127, 0, 0, 1], 12000))
            .await?;

        self.rt
            .spawn(async move {
                while let Some(msg) = sender_rx.recv().await {
                    msg.pack(&mut send_buf).expect("message packed");

                    tx.send_reliable(0, &send_buf).await.expect("message sent");
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

        let player_actor = Actor(0);
        let mut center_chunk = Chunk {
            position: [0, 0, 0],
            dimension: 0,
        };

        mbcc.set(
            BlockClass(1),
            Model::Cube(Cube {
                textures: [1, 1, 1, 1, 1, 0],
            }),
        );

        let mut last_render_time = Instant::now();

        let mut position_system = PositionSystem::new();
        let mut direct_control_system = DirectControl::new(player_actor, 10.0, 0.4);

        let mut pac = PositionActorComponent::new();
        let mut vac = VelocityActorComponent::new();
        let mut fac = FacingActorComponent::new();

        pac.set(
            player_actor,
            Position {
                vector: Vec3::new(8.0, 8.0, 24.0),
            },
        );
        vac.set(
            player_actor,
            Velocity {
                vector: Vec3::new(0.0, 0.0, 0.0),
            },
        );
        fac.set(
            player_actor,
            Facing {
                yaw: 0.0,
                pitch: 0.0,
            },
        );

        let mut render_system =
            RenderSystem::new(instance, surface, surface_size, &center_chunk, &pac, &fac).await;

        let mut stream = Timer::interval(Duration::from_millis(20))
            .map(|_| Event::Process)
            .or(window_event_rx.stream())
            .or(event_rx);

        while let Some(event) = stream.next().await {
            match event {
                Event::Process => {
                    let elapsed = last_render_time.elapsed();
                    last_render_time = Instant::now();

                    // TODO consider what should really be unblocked?
                    // let time_test = Instant::now();
                    position_system.process(elapsed, &center_chunk, &cbc, &mut pac, &vac);
                    position_system.post_movement(
                        &player_actor,
                        &mut center_chunk,
                        &mut pac,
                        &event_tx,
                    );
                    direct_control_system.process(elapsed, &mut vac, &mut fac);
                    render_system.update(&center_chunk, &pac, &fac);
                    render_system.render()?;
                    // log::error!("Elapsed: {:?}", time_test.elapsed());
                },
                Event::Key { input } => {
                    direct_control_system.process_keyboard(&input);
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
                        ClientAccept::ClassBlockComponent { coords, value } => {
                            let chunk = Chunk {
                                position: coords.position,
                                dimension: coords.dimension,
                            };

                            let chunk_blocks = value.into_iter().map(|c| BlockClass(c)).collect();

                            if let Some(ChunkStatus::Loading) = scc.get_chunk(&chunk) {
                                cbc.insert_chunk(chunk, Blocks::new(chunk_blocks));
                                scc.insert_chunk(chunk, ChunkStatus::Active);
                                let _ = event_tx.send(Event::DrawChunk { chunk });
                            }
                        },
                    }
                },
                Event::NearbyChunksUpdate => {
                    let new_chunks = (-5 ..= 5)
                        .into_iter()
                        .map(|z| (-5 ..= 5).into_iter().map(move |y| (y, z)))
                        .flatten()
                        .map(|(y, z)| (-5 ..= 5).into_iter().map(move |x| (x, y, z)))
                        .flatten()
                        .map(|(x, y, z)| {
                            Chunk {
                                position: [
                                    center_chunk.position[0] + x,
                                    center_chunk.position[1] + y,
                                    center_chunk.position[2] + z,
                                ],
                                dimension: center_chunk.dimension,
                            }
                        })
                        .collect::<BTreeSet<_>>();

                    let add_chunks: Vec<_> = new_chunks
                        .iter()
                        .filter(|chunk| cbc.get_chunk(chunk).is_none())
                        .inspect(|chunk| scc.insert_chunk(**chunk, ChunkStatus::Loading))
                        .map(|chunk| {
                            ChunkRequest {
                                position: chunk.position,
                                dimension: chunk.dimension,
                            }
                        })
                        .collect();

                    sender_tx
                        .send(ServerAccept::GetChunksBlocks { coords: add_chunks })
                        .expect("initial request sent to sender");

                    let remove_chunks = cbc
                        .iter()
                        .map(|(chunk, _)| *chunk)
                        .filter(|chunk| !new_chunks.contains(chunk))
                        .collect::<Vec<_>>();

                    for chunk in remove_chunks {
                        scc.remove_chunk(&chunk);
                        cbc.remove_chunk(&chunk);
                        let _ = event_tx.send(Event::DrawChunk { chunk });
                    }
                },
                Event::DrawChunk { chunk } => {
                    render_system.build_chunk(&chunk, &cbc, &mbcc);
                },
                Event::Shutdown => break,
            }
        }

        Ok(())
    }
}
