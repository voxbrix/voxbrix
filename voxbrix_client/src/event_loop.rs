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

pub enum Event {
    Process,
    // Render,
    Key { input: WinitKeyboardInput },
    MouseMove { horizontal: f32, vertical: f32 },
    WindowResize { new_size: PhysicalSize<u32> },
    Shutdown,
}

pub struct EventLoop {
    pub window: WindowHandle,
}

impl EventLoop {
    pub async fn run(self) -> Result<()> {
        let WindowHandle {
            instance,
            surface,
            size: surface_size,
            event_rx: window_event_rx,
        } = self.window;

        let player_actor = Actor(0);

        let mut mbcc = ModelBlockClassComponent::new();

        mbcc.set(
            BlockClass(1),
            Model::Cube(Cube {
                textures: [1, 1, 1, 1, 1, 0],
            }),
        );

        let mut cbc = ClassBlockComponent::new();

        let chunk = Chunk {
            position: [0, 0, 0],
            dimention: 0,
        };
        let mut chunk_blocks = vec![BlockClass(1); 4096];
        chunk_blocks[3005] = BlockClass(0);
        chunk_blocks[1005] = BlockClass(0);
        chunk_blocks[405] = BlockClass(0);
        chunk_blocks[4005] = BlockClass(0);
        chunk_blocks[4095] = BlockClass(0);
        chunk_blocks[4094] = BlockClass(0);

        cbc.insert_chunk(chunk, Blocks::new(chunk_blocks));

        let chunk = Chunk {
            position: [0, 0, 1],
            dimention: 0,
        };

        let mut chunk_blocks = vec![BlockClass(0); 4096];
        chunk_blocks[0] = BlockClass(1);
        chunk_blocks[255] = BlockClass(1);
        chunk_blocks[254] = BlockClass(1);
        chunk_blocks[253] = BlockClass(1);
        chunk_blocks[256] = BlockClass(1);
        chunk_blocks[2000] = BlockClass(1);
        chunk_blocks[2256] = BlockClass(1);

        cbc.insert_chunk(chunk, Blocks::new(chunk_blocks));

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
