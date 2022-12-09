use crate::{
    component::{
        actor::{
            facing::{
                Facing,
                FacingActorComponent,
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
        block_class::BlockClass,
        chunk::Chunk,
    },
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
    pack::Pack,
    ChunkData,
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

        mbcc.set(
            BlockClass(1),
            Model::Cube(Cube {
                textures: [1, 1, 1, 1, 1, 0],
            }),
        );

        let mut last_render_time = Instant::now();

        let mut position_system = PositionSystem::new();
        let mut direct_control_system = DirectControl::new(player_actor, 10.0, 0.4);

        let mut gpac = GlobalPositionActorComponent::new();
        let mut vac = VelocityActorComponent::new();
        let mut fac = FacingActorComponent::new();

        gpac.insert(
            player_actor,
            GlobalPosition {
                chunk: Chunk {
                    position: [0, 0, 0].into(),
                    dimension: 0,
                },
                offset: Vec3::new([0.0, 0.0, 4.0]),
            },
        );
        vac.insert(
            player_actor,
            Velocity {
                vector: Vec3::new([0.0, 0.0, 0.0]),
            },
        );
        fac.insert(
            player_actor,
            Facing {
                yaw: 0.0,
                pitch: 0.0,
            },
        );

        let mut render_system =
            RenderSystem::new(instance, surface, surface_size, &gpac, &fac).await;

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
                    position_system.process(elapsed, &cbc, &mut gpac, &vac);
                    let player_position = gpac.get(&player_actor).unwrap();
                    sender_tx.send(ServerAccept::PlayerPosition {
                        position: player_position.clone(),
                    });
                    direct_control_system.process(elapsed, &mut vac, &mut fac);
                    render_system.update(&gpac, &fac);
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
                        ClientAccept::ChunkData(ChunkData {
                            chunk,
                            block_classes,
                        }) => {
                            cbc.insert_chunk(chunk, block_classes);
                            scc.insert(chunk, ChunkStatus::Active);
                            let _ = event_tx.send(Event::DrawChunk { chunk });
                        },
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