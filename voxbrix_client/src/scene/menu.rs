use crate::{
    scene::{
        game::GameSceneParameters,
        SceneSwitch,
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
    future,
    StreamExt as _,
};
use iced_wgpu::{
    Backend,
    Renderer,
    Settings,
    Viewport,
};
use iced_winit::{
    alignment,
    program,
    renderer,
    widget::{
        button,
        text,
        text_input,
        Column,
        Row,
        Text,
    },
    Alignment,
    Clipboard,
    Color,
    Command,
    Debug,
    Element,
    Length,
    Program,
    Size,
};
use std::time::Duration;
use voxbrix_common::{
    messages::{
        client::InitResponse,
        server::InitRequest,
    },
    pack::PackZip,
    stream::StreamExt as _,
};
use voxbrix_protocol::client::Client;
use winit::{
    dpi::PhysicalPosition,
    event::ModifiersState,
};

enum MainMenuAction {
    Submit,
    Exit,
}

pub struct MainMenu {
    error_message: String,
    server_address: String,
    username: String,
    password: String,
    command: Option<MainMenuAction>,
}

#[derive(Debug, Clone)]
pub enum Message {
    UpdateServerAddress(String),
    UpdateUsername(String),
    UpdatePassword(String),
    Error(String),
    Submit,
    Exit,
}

impl MainMenu {
    pub fn new() -> MainMenu {
        MainMenu {
            error_message: String::default(),
            server_address: String::default(),
            username: String::default(),
            password: String::default(),
            command: None,
        }
    }
}

impl Program for MainMenu {
    type Message = Message;
    type Renderer = Renderer;

    fn update(&mut self, message: Message) -> Command<Message> {
        self.command = None;
        match message {
            Message::UpdateServerAddress(s) => {
                self.server_address = s;
                self.error_message = String::default();
            },
            Message::UpdateUsername(s) => {
                self.username = s;
                self.error_message = String::default();
            },
            Message::UpdatePassword(s) => {
                self.password = s;
                self.error_message = String::default();
            },
            Message::Error(s) => {
                self.error_message = s;
            },
            Message::Submit => {
                self.command = Some(MainMenuAction::Submit);
            },
            Message::Exit => {
                self.command = Some(MainMenuAction::Exit);
            },
        }

        Command::none()
    }

    fn view(&self) -> Element<Message, Renderer> {
        Row::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Alignment::End)
            .push(
                Column::new()
                    .width(Length::Fill)
                    .align_items(Alignment::End)
                    .push(
                        Column::new()
                            .padding(10)
                            .spacing(10)
                            .push(Text::new("voxbrix").style(Color::WHITE))
                            .push(Text::new(&self.error_message).style(Color {
                                r: 1.0,
                                g: 0.5,
                                b: 0.5,
                                a: 1.0,
                            }))
                            .push(text_input(
                                "Server address",
                                &self.server_address,
                                Message::UpdateServerAddress,
                            ))
                            .push(text_input(
                                "Username",
                                &self.username,
                                Message::UpdateUsername,
                            ))
                            .push(text_input(
                                "Password",
                                &self.password,
                                Message::UpdatePassword,
                            ))
                            .push(
                                button(
                                    text("Play")
                                        .horizontal_alignment(alignment::Horizontal::Center),
                                )
                                .padding(10)
                                .width(Length::Units(100))
                                .on_press(Message::Submit),
                            )
                            .push(
                                button(
                                    text("Exit")
                                        .horizontal_alignment(alignment::Horizontal::Center),
                                )
                                .padding(10)
                                .width(Length::Units(100))
                                .on_press(Message::Exit),
                            ),
                    ),
            )
            .into()
    }
}

enum Event {
    Process,
    Input(InputEvent),
}

pub struct MenuScene<'a> {
    pub rt: &'a LocalExecutor<'a>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
}

impl MenuScene<'_> {
    pub async fn run(self) -> Result<SceneSwitch> {
        let physical_size = self.window_handle.window.inner_size();

        let viewport = Viewport::with_physical_size(
            Size::new(physical_size.width, physical_size.height),
            self.window_handle.window.scale_factor(),
        );

        let format = self
            .render_handle
            .surface
            .get_supported_formats(&self.render_handle.adapter)[0];

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: physical_size.width,
            height: physical_size.height,
            // Fifo makes SurfaceTexture::present() block
            // which is bad for current rendering implementation
            present_mode: wgpu::PresentMode::Mailbox,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
        };

        self.render_handle
            .surface
            .configure(&self.render_handle.device, &config);

        let _ = self
            .window_handle
            .window
            .set_cursor_grab(winit::window::CursorGrabMode::None);
        self.window_handle.window.set_cursor_visible(true);

        let mut renderer = Renderer::new(Backend::new(
            &self.render_handle.device,
            Settings::default(),
            format,
        ));

        let mut debug = Debug::new();

        let mut menu = program::State::new(
            MainMenu::new(),
            viewport.logical_size(),
            &mut renderer,
            &mut debug,
        );

        let mut cursor_position = PhysicalPosition::new(-1.0, -1.0);
        let mut modifiers = ModifiersState::default();
        let mut clipboard = Clipboard::connect(&self.window_handle.window);
        let mut resized = true;

        let mut staging_belt = wgpu::util::StagingBelt::new(5 * 1024);

        let mut stream = Timer::interval(Duration::from_millis(20))
            .map(|_| Event::Process)
            .or_ff(self.window_handle.event_rx.stream().map(Event::Input));

        // let server = match program.server_address.parse() {
        // Ok(s) => s,
        // Err(_) => {
        // },
        // }

        while let Some(event) = stream.next().await {
            match event {
                Event::Process => {
                    let action = async {
                        let program = menu.program();
                        if let Some(action) = &program.command {
                            match action {
                                MainMenuAction::Submit => {
                                    let connect_result = async {
                                        let socket: std::net::SocketAddr = ([0, 0, 0, 0], 0).into();
                                        let server: std::net::SocketAddr =
                                            program.server_address.parse().map_err(|_| {
                                                "Incorrect server socket address format"
                                            })?;

                                        let (mut tx, mut rx) = Client::bind(socket)
                                            .map_err(|_| "Unable to bind socket")?
                                            .connect(server)
                                            .await
                                            .map_err(|_| "Connection error")?;

                                        let req_result = async {
                                            tx.send_reliable(
                                                0,
                                                &InitRequest {
                                                    username: program.username.to_owned(),
                                                    password: program
                                                        .password
                                                        .as_bytes()
                                                        .to_owned(),
                                                }
                                                .pack_to_vec(),
                                            )
                                            .await
                                            .map_err(|_| "Unable to send initialization request")
                                        };

                                        let init_response = async {
                                            let mut recv_buf = Vec::new();
                                            let (_channel, bytes) =
                                                rx.recv(&mut recv_buf).await.map_err(|_| {
                                                    "Unable to get initialization response"
                                                })?;
                                            InitResponse::unpack(&bytes).map_err(|_| {
                                                "Unable to unpack initialization response"
                                            })
                                        };

                                        let (req_result, init_response) =
                                            future::zip(req_result, init_response).await;
                                        req_result?;
                                        let init_response = init_response?;

                                        let (player_actor, player_ticket_radius) =
                                            match init_response {
                                                InitResponse::Success {
                                                    actor,
                                                    player_ticket_radius,
                                                } => (actor, player_ticket_radius),
                                                InitResponse::Failure(_err) => {
                                                    return Err("Incorrect password");
                                                },
                                            };
                                        return Ok::<_, &str>(GameSceneParameters {
                                            connection: (tx, rx),
                                            player_actor,
                                            player_ticket_radius,
                                        });
                                    };

                                    match connect_result.await {
                                        Ok(parameters) => {
                                            return Some(SceneSwitch::Game { parameters });
                                        },
                                        Err(err) => {
                                            menu.queue_message(Message::Error(err.to_owned()));
                                            return None;
                                        },
                                    };
                                },
                                MainMenuAction::Exit => {
                                    return Some(SceneSwitch::Exit);
                                },
                            }
                        }
                        None
                    };

                    if let Some(action) = action.await {
                        return Ok(action);
                    }

                    match self.render_handle.surface.get_current_texture() {
                        Ok(frame) => {
                            let _ = menu.update(
                                viewport.logical_size(),
                                iced_winit::conversion::cursor_position(
                                    cursor_position,
                                    viewport.scale_factor(),
                                ),
                                &mut renderer,
                                &iced_wgpu::Theme::Dark,
                                &renderer::Style {
                                    text_color: Color::WHITE,
                                },
                                &mut clipboard,
                                &mut debug,
                            );

                            let mut encoder = self.render_handle.device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor { label: None },
                            );

                            let view = frame
                                .texture
                                .create_view(&wgpu::TextureViewDescriptor::default());

                            renderer.with_primitives(|backend, primitive| {
                                backend.present(
                                    &self.render_handle.device,
                                    &mut staging_belt,
                                    &mut encoder,
                                    &view,
                                    primitive,
                                    &viewport,
                                    &debug.overlay(),
                                );
                            });

                            staging_belt.finish();
                            self.render_handle.queue.submit(Some(encoder.finish()));
                            frame.present();

                            // Update the mouse cursor
                            self.window_handle.window.set_cursor_icon(
                                iced_winit::conversion::mouse_interaction(menu.mouse_interaction()),
                            );

                            // And recall staging buffers
                            staging_belt.recall();
                        },
                        Err(error) => {
                            panic!("Swapchain error: {}. Rendering cannot continue.", error);
                        },
                    }
                },
                Event::Input(event) => {
                    if let InputEvent::WindowEvent { event } = event {
                        match event {
                            WindowEvent::CursorMoved { position, .. } => {
                                cursor_position = position;
                            },
                            WindowEvent::ModifiersChanged(new_modifiers) => {
                                modifiers = new_modifiers;
                            },
                            WindowEvent::Resized(_) => {
                                resized = true;
                            },
                            WindowEvent::CloseRequested => {
                                return Ok(SceneSwitch::Exit);
                            },
                            _ => {},
                        }
                        if let Some(event) =
                            event.to_iced(self.window_handle.window.scale_factor(), modifiers)
                        {
                            menu.queue_event(event);
                        }
                    }
                },
            }
        }

        Ok(SceneSwitch::Exit)
    }
}
