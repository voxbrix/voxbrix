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
use async_io::Timer;
use futures_lite::stream::StreamExt;
use std::time::{
    Duration,
    Instant,
};
use winit::{
    dpi::PhysicalSize,
    event::KeyboardInput as WinitKeyboardInput,
};
use voxbrix_protocol::client::Client;
use voxbrix_messages::{
    Pack,
    Chunk as ChunkMessage,
    server::ServerAccept,
    client::ClientAccept,
};
use async_executor::LocalExecutor;

pub enum Event {
    Process,
    // Render,
    Key { input: WinitKeyboardInput },
    MouseMove { horizontal: f32, vertical: f32 },
    WindowResize { new_size: PhysicalSize<u32> },
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

        let mut send_buf = Vec::new();

        let (mut tx, mut rx) = Client::bind(([127, 0, 0, 1], 12001))?
            .connect(([127, 0, 0, 1], 12000)).await?;

        let request = ServerAccept::GetChunksBlocks {
            coords: vec![
                ChunkMessage {
                    position: [0, 0, -1],
                    dimention: 0,
                },
                ChunkMessage {
                    position: [0, 0, 0],
                    dimention: 0,
                },
            ],
        };

        request.pack(&mut send_buf)
            .expect("message packed");

        self.rt.spawn(async move {
            tx.send_reliable(0, &send_buf).await
                .expect("message sent");
        }).detach();

        let mut recv_buf = Vec::new();
        let mut cbc = ClassBlockComponent::new();

        for _ in 0 .. 2 {
            let (_channel, msg) = rx.recv(&mut recv_buf).await
                .expect("message receive");

            let resp = ClientAccept::unpack(msg)
                .expect("message unpacked");

            match resp {
                ClientAccept::ClassBlockComponent { coords, value } => {
                    let chunk = Chunk { position: coords.position, dimention: coords.dimention };
                    let chunk_blocks = value.into_iter()
                        .map(|c| BlockClass(c))
                        .collect();

                    cbc.insert_chunk(chunk, Blocks::new(chunk_blocks));
                },
            };
        }

        let player_actor = Actor(0);

        let mut mbcc = ModelBlockClassComponent::new();

        mbcc.set(
            BlockClass(1),
            Model::Cube(Cube {
                textures: [1, 1, 1, 1, 1, 0],
            }),
        );

        let mut last_render_time = Instant::now();

        let mut position_system = PositionSystem::new();
        let mut direct_control_system = DirectControl::new(player_actor, 1000.0, 0.4);

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
            RenderSystem::new(instance, surface, surface_size, &pac, &fac, &cbc, &mbcc).await;

        let mut stream = window_event_rx
            .stream()
            .or(Timer::interval(Duration::from_millis(20)).map(|_| Event::Process));

        while let Some(event) = stream.next().await {
            match event {
                Event::Process => {
                    let elapsed = last_render_time.elapsed();
                    last_render_time = Instant::now();

                    // TODO consider what should really be unblocked?
                    // let time_test = Instant::now();
                    position_system.process(elapsed, &cbc, &mut pac, &vac);
                    direct_control_system.process(elapsed, &mut vac, &mut fac);
                    render_system.update(&pac, &fac);
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
                Event::Shutdown => break,
            }
        }

        Ok(())
    }
}
