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
            ModelDescriptor,
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
        block_texture_loading::BlockTextureLoadingSystem,
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
    CONNECTION_TIMEOUT,
};
use anyhow::Result;
use async_executor::LocalExecutor;
use async_io::Timer;
use futures_lite::{
    future::FutureExt,
    stream::StreamExt,
};
use local_channel::mpsc::Sender as ChannelSender;
use log::error;
use rayon::iter::{
    IntoParallelIterator,
    ParallelIterator,
};
use std::{
    io::ErrorKind as StdIoErrorKind,
    time::{
        Duration,
        Instant,
    },
};
use voxbrix_common::{
    component::block::sky_light::SkyLightBlockComponent,
    math::Vec3,
    messages::{
        client::ClientAccept,
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

        // Should be dropped when the loop ends
        let _recv_task = self.rt.spawn(async move {
            let mut buf = Vec::new();
            loop {
                let data = match rx.recv(&mut buf).await {
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
        let block_texture_loading_system = BlockTextureLoadingSystem::load_data().await?;

        let mut scc = StatusChunkComponent::new();

        let mut cbc = ClassBlockComponent::new();
        let mut slbc = SkyLightBlockComponent::new();
        let mut mbcc = ModelBlockClassComponent::new();

        block_class_loading_system.load_component(
            "model",
            &mut mbcc,
            |desc: ModelDescriptor| {
                match desc {
                    ModelDescriptor::Cube {
                        textures: textures_desc,
                    } => {
                        let mut textures = [0; 6];

                        for (i, texture) in textures.iter_mut().enumerate() {
                            *texture = match textures_desc[i].as_str() {
                                "grass" => 1,
                                "dirt" => 2,
                                name => {
                                    return Err(anyhow::Error::msg(format!(
                                        "Texture not found: {}",
                                        name
                                    )))
                                },
                            }
                        }

                        Ok(Model::Cube(Cube { textures }))
                    },
                }
            },
        )?;

        let mut last_render_time = Instant::now();

        let mut position_system = PositionSystem::new();
        let mut direct_control_system = DirectControl::new(player_actor, 10.0, 0.4);
        let chunk_presence_system = ChunkPresenceSystem::new();
        let sky_light_system = SkyLightSystem::new();

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
            block_texture_loading_system.textures,
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
                                .map(|blocks| blocks.get(block) == &BlockClass(1))
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
                                    if let Some(winit::event::VirtualKeyCode::Escape) =
                                        input.virtual_keycode
                                    {
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

                                                if let Some((chunk, block, _side)) =
                                                    PositionSystem::get_target_block(
                                                        position,
                                                        orientation,
                                                        |chunk, block| {
                                                            cbc.get_chunk(&chunk)
                                                                .map(|blocks| {
                                                                    blocks.get(block)
                                                                        == &BlockClass(1)
                                                                })
                                                                .unwrap_or(false)
                                                        },
                                                    )
                                                {
                                                    let _ = reliable_tx.send(
                                                        ServerAccept::AlterBlock {
                                                            chunk,
                                                            block,
                                                            block_class: BlockClass(0),
                                                        },
                                                    );
                                                }
                                            },
                                            MouseButton::Right => {
                                                let position = gpac.get(&player_actor).unwrap();
                                                let orientation = oac.get(&player_actor).unwrap();

                                                if let Some((chunk, block, side)) =
                                                    PositionSystem::get_target_block(
                                                        position,
                                                        orientation,
                                                        |chunk, block| {
                                                            cbc.get_chunk(&chunk)
                                                                .map(|blocks| {
                                                                    blocks.get(block)
                                                                        == &BlockClass(1)
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
                                                            block_class: BlockClass(1),
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
                            cbc.insert_chunk(chunk, block_classes);
                            scc.insert(chunk, ChunkStatus::Active);

                            let _ = event_tx.send(Event::DrawChunk(chunk));

                            calc_light(&sky_light_system, chunk, &cbc, &mut slbc, &event_tx);
                        },
                        ClientAccept::AlterBlock {
                            chunk,
                            block,
                            block_class,
                        } => {
                            if let Some(block_class_ref) =
                                cbc.get_mut_chunk(&chunk).map(|c| c.get_mut(block))
                            {
                                *block_class_ref = block_class;

                                let _ = event_tx.send(Event::DrawChunk(chunk));

                                calc_light(&sky_light_system, chunk, &cbc, &mut slbc, &event_tx);
                            }
                        },
                    }
                },
                Event::DrawChunk(chunk) => {
                    // TODO: use separate *set* as a queue and take up to num_cpus each time the event
                    // comes
                    unblock!((render_system, cbc, mbcc, slbc) {
                        render_system.build_chunk(&chunk, &cbc, &mbcc, &slbc);
                    });
                },
            }
        }

        Ok(SceneSwitch::Menu)
    }
}

// TODO move to more appropriate place
fn calc_light(
    sky_light_system: &SkyLightSystem,
    chunk: Chunk,
    cbc: &ClassBlockComponent,
    slbc: &mut SkyLightBlockComponent,
    event_tx: &ChannelSender<Event>,
) {
    let (light_component, chunks_to_recalc) =
        sky_light_system.recalculate_chunk(chunk, None, &cbc, &slbc);

    let mut chunks_to_recalc: std::collections::BTreeSet<_> =
        chunks_to_recalc.into_iter().collect();

    slbc.insert_chunk(chunk, light_component);

    loop {
        let results = chunks_to_recalc
            .iter()
            .filter_map(|chunk| Some((chunk, slbc.remove_chunk(chunk)?)))
            .collect::<Vec<_>>();

        if results.is_empty() {
            break;
        }

        let results = results
            .into_par_iter()
            .map(|(chunk, old_light_component)| {
                let (light_component, chunks_to_recalc) = sky_light_system.recalculate_chunk(
                    *chunk,
                    Some(old_light_component),
                    &cbc,
                    &slbc,
                );

                (*chunk, light_component, chunks_to_recalc)
            })
            .collect::<Vec<_>>();

        let expansion =
            results
                .into_iter()
                .flat_map(|(chunk, light_component, chunks_to_recalc)| {
                    slbc.insert_chunk(chunk, light_component);
                    let _ = event_tx.send(Event::DrawChunk(chunk));
                    chunks_to_recalc.into_iter()
                });

        chunks_to_recalc.clear();
        chunks_to_recalc.extend(expansion);
    }
}
