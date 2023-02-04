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
    CONNECTION_TIMEOUT,
};
use anyhow::Result;
use argon2::Argon2;
use async_executor::LocalExecutor;
use async_io::Timer;
use futures_lite::{
    future,
    FutureExt as _,
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
        Space,
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
use k256::ecdsa::{
    signature::{
        Signature as _,
        Signer as _,
        Verifier as _,
    },
    Signature,
    SigningKey,
    VerifyingKey,
};
use local_channel::mpsc::Sender;
use std::time::Duration;
use voxbrix_common::{
    messages::{
        client::{
            InitData,
            InitResponse,
            LoginResult,
            RegisterResult,
        },
        server::{
            InitRequest,
            LoginRequest,
            RegisterRequest,
        },
    },
    pack::Pack,
    stream::StreamExt as _,
};
use voxbrix_protocol::client::{
    Client,
    Connection,
};
use winit::{
    dpi::PhysicalPosition,
    event::ModifiersState,
};

pub enum MainMenuAction {
    Submit,
    Exit,
}

#[derive(Debug, Clone, Copy)]
pub enum FormType {
    Login,
    Registration,
}

pub struct MainMenu {
    form_type: FormType,
    error_message: String,
    server_address: String,
    username: String,
    password: String,
    password_confirmation: String,
    event_tx: Sender<MainMenuAction>,
}

#[derive(Debug, Clone)]
pub enum Message {
    UpdateServerAddress(String),
    UpdateUsername(String),
    UpdatePassword(String),
    UpdatePasswordConfirmation(String),
    Error(String),
    SwitchForm(FormType),
    Submit,
    Exit,
}

impl MainMenu {
    pub fn new(event_tx: Sender<MainMenuAction>) -> MainMenu {
        MainMenu {
            form_type: FormType::Login,
            error_message: String::default(),
            server_address: String::default(),
            username: String::default(),
            password: String::default(),
            password_confirmation: String::default(),
            event_tx,
        }
    }
}

impl Program for MainMenu {
    type Message = Message;
    type Renderer = Renderer;

    fn update(&mut self, message: Message) -> Command<Message> {
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
            Message::UpdatePasswordConfirmation(s) => {
                self.password_confirmation = s;
                self.error_message = String::default();
            },
            Message::Error(s) => {
                self.error_message = s;
            },
            Message::SwitchForm(form_type) => {
                self.form_type = form_type;
            },
            Message::Submit => {
                let _ = self.event_tx.send(MainMenuAction::Submit);
            },
            Message::Exit => {
                let _ = self.event_tx.send(MainMenuAction::Exit);
            },
        }

        Command::none()
    }

    fn view(&self) -> Element<Message, Renderer> {
        let mut form = Column::new()
            .align_items(Alignment::Center)
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
            ));

        if let FormType::Registration = self.form_type {
            form = form.push(text_input(
                "Password confirmation",
                &self.password_confirmation,
                Message::UpdatePasswordConfirmation,
            ));
        }

        form = form.push(
            button(text("Submit").horizontal_alignment(alignment::Horizontal::Center))
                .padding(10)
                .width(Length::Units(100))
                .on_press(Message::Submit),
        );

        Column::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(10)
            .push(
                Row::new()
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_items(Alignment::Center)
                    .push(Space::new(Length::FillPortion(1), Length::Fill))
                    .push(
                        Column::new()
                            .width(Length::FillPortion(1))
                            .align_items(Alignment::Center)
                            .push(Space::new(Length::Fill, Length::Fill))
                            .push(form)
                            .push(Space::new(Length::Fill, Length::Fill)),
                    )
                    .push(Space::new(Length::FillPortion(1), Length::Fill)),
            )
            .push(
                Row::new()
                    .width(Length::Fill)
                    .height(Length::Shrink)
                    .align_items(Alignment::Center)
                    .push(match self.form_type {
                        FormType::Login => {
                            button(
                                text("Registration")
                                    .horizontal_alignment(alignment::Horizontal::Center),
                            )
                            .padding(10)
                            .width(Length::Units(150))
                            .on_press(Message::SwitchForm(FormType::Registration))
                        },
                        FormType::Registration => {
                            button(
                                text("Login").horizontal_alignment(alignment::Horizontal::Center),
                            )
                            .padding(10)
                            .width(Length::Units(150))
                            .on_press(Message::SwitchForm(FormType::Login))
                        },
                    })
                    .push(Space::new(Length::Fill, Length::Shrink))
                    .push(
                        button(text("Exit").horizontal_alignment(alignment::Horizontal::Center))
                            .padding(10)
                            .width(Length::Units(150))
                            .on_press(Message::Exit),
                    ),
            )
            .into()
    }
}

enum Event {
    Process,
    Input(InputEvent),
    Action(MainMenuAction),
}

pub struct MenuScene<'a> {
    pub rt: &'a LocalExecutor<'a>,
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
}

impl MenuScene<'_> {
    pub async fn run(self) -> Result<SceneSwitch> {
        let physical_size = self.window_handle.window.inner_size();

        let mut viewport = Viewport::with_physical_size(
            Size::new(physical_size.width, physical_size.height),
            self.window_handle.window.scale_factor(),
        );

        let format = self
            .render_handle
            .surface
            .get_supported_formats(&self.render_handle.adapter)[0];

        let present_mode = self
            .render_handle
            .surface
            .get_supported_present_modes(&self.render_handle.adapter)
            .into_iter()
            .find(|pm| *pm == wgpu::PresentMode::Mailbox)
            .unwrap_or(wgpu::PresentMode::Immediate);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: physical_size.width,
            height: physical_size.height,
            // Fifo makes SurfaceTexture::present() block
            // which is bad for current rendering implementation
            present_mode,
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

        let (event_tx, event_rx) = local_channel::mpsc::channel();

        let mut menu = program::State::new(
            MainMenu::new(event_tx),
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
            .or_ff(self.window_handle.event_rx.stream().map(Event::Input))
            .or_ff(event_rx.map(Event::Action));

        // let server = match program.server_address.parse() {
        // Ok(s) => s,
        // Err(_) => {
        // },
        // }

        while let Some(event) = stream.next().await {
            match event {
                Event::Process => {
                    if resized {
                        let physical_size = self.window_handle.window.inner_size();
                        viewport = Viewport::with_physical_size(
                            Size::new(physical_size.width, physical_size.height),
                            self.window_handle.window.scale_factor(),
                        );

                        let config = wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format,
                            width: physical_size.width,
                            height: physical_size.height,
                            // Fifo makes SurfaceTexture::present() block
                            // which is bad for current rendering implementation
                            present_mode,
                            alpha_mode: wgpu::CompositeAlphaMode::Auto,
                        };

                        self.render_handle
                            .surface
                            .configure(&self.render_handle.device, &config);
                    }

                    let frame = self
                        .render_handle
                        .surface
                        .get_current_texture()
                        .expect("getting texture");

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

                    let mut encoder = self
                        .render_handle
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

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
                Event::Action(action) => {
                    let program = menu.program();
                    match action {
                        MainMenuAction::Submit => {
                            let connection_result = async {
                                let form_type = program.form_type;
                                let mut tx_buffer = Vec::new();
                                let mut rx_buffer = Vec::new();
                                let socket: std::net::SocketAddr = ([0, 0, 0, 0], 0).into();
                                let server: std::net::SocketAddr =
                                    program
                                        .server_address
                                        .parse()
                                        .map_err(|_| "Incorrect server socket address format")?;

                                let Connection {
                                    self_key,
                                    peer_key,
                                    sender: mut tx,
                                    receiver: mut rx,
                                } = Client::bind(socket)
                                    .map_err(|_| "Unable to bind socket")?
                                    .connect(server)
                                    .await
                                    .map_err(|_| "Connection error")?;

                                let (_req_result, response) = async {
                                    Ok(future::zip(
                                        async {
                                            match form_type {
                                                FormType::Login => {
                                                    InitRequest::Login.pack(&mut tx_buffer)
                                                },
                                                FormType::Registration => {
                                                    InitRequest::Register.pack(&mut tx_buffer)
                                                },
                                            };

                                            tx.send_reliable(0, &tx_buffer).await.map_err(|_| {
                                                "Unable to send initialization request"
                                            })
                                        },
                                        async {
                                            let (_channel, bytes) =
                                                rx.recv(&mut rx_buffer).await.map_err(|_| {
                                                    "Unable to get initialization response"
                                                })?;
                                            InitResponse::unpack(bytes).map_err(|_| {
                                                "Unable to unpack initialization response"
                                            })
                                        },
                                    )
                                    .await)
                                }
                                .or(async {
                                    Timer::interval(CONNECTION_TIMEOUT).await;
                                    Err("Connection timeout")
                                })
                                .await?;

                                let InitResponse {
                                    public_key: server_key,
                                    key_signature,
                                } = response?;

                                let server_key = VerifyingKey::from_sec1_bytes(&server_key)
                                    .map_err(|_| "Server provided incorrect public key")?;

                                let key_signature = Signature::from_bytes(&key_signature)
                                    .map_err(|_| "Server provided incorrect signature")?;

                                server_key.verify(&peer_key, &key_signature).map_err(|_| {
                                    "Server signature does not match the public key provided"
                                })?;

                                if let FormType::Registration = form_type {
                                    if program.password != program.password_confirmation {
                                        return Err(
                                            "Password and password confirmation do not match"
                                        );
                                    }
                                }

                                let mut signing_key = [0; 32];
                                Argon2::default()
                                    .hash_password_into(
                                        program.password.as_bytes(),
                                        program.username.as_bytes(),
                                        &mut signing_key,
                                    )
                                    .unwrap();

                                let signing_key = SigningKey::from_bytes(&signing_key)
                                    .expect("signing key derive");

                                match form_type {
                                    FormType::Login => {
                                        let signature: Signature = signing_key.sign(&self_key);
                                        LoginRequest {
                                            username: program.username.clone(),
                                            key_signature: signature.as_bytes().try_into().unwrap(),
                                        }
                                        .pack(&mut tx_buffer)
                                    },
                                    FormType::Registration => {
                                        RegisterRequest {
                                            username: program.username.clone(),
                                            public_key: signing_key
                                                .verifying_key()
                                                .to_bytes()
                                                .try_into()
                                                .unwrap(),
                                        }
                                        .pack(&mut tx_buffer)
                                    },
                                }

                                let (_req_result, response_bytes) = async {
                                    Ok(future::zip(
                                        async {
                                            tx.send_reliable(0, &tx_buffer)
                                                .await
                                                .map_err(|_| "Unable to send initial data request")
                                        },
                                        async {
                                            let (_channel, bytes) =
                                                rx.recv(&mut rx_buffer).await.map_err(|_| {
                                                    "Unable to get initial data response"
                                                })?;
                                            Ok::<&mut [u8], &str>(bytes)
                                        },
                                    )
                                    .await)
                                }
                                .or(async {
                                    Timer::interval(CONNECTION_TIMEOUT).await;
                                    Err("Connection timeout")
                                })
                                .await?;

                                let response_bytes = response_bytes?;

                                let init_data = match form_type {
                                    FormType::Login => {
                                        let res = LoginResult::unpack(&response_bytes)
                                            .map_err(|_| "Incorrect response format")?;

                                        match res {
                                            LoginResult::Success(data) => data,
                                            LoginResult::Failure(_) => {
                                                // TODO: display actual error
                                                return Err("Incorrect login credentials");
                                            },
                                        }
                                    },
                                    FormType::Registration => {
                                        let res = RegisterResult::unpack(response_bytes)
                                            .map_err(|_| "Incorrect response format")?;

                                        match res {
                                            RegisterResult::Success(data) => data,
                                            RegisterResult::Failure(_) => {
                                                // TODO: display actual error
                                                return Err("Username already taken");
                                            },
                                        }
                                    },
                                };

                                Ok((tx, rx, init_data))
                            };

                            match connection_result.await {
                                Ok((tx, rx, init_data)) => {
                                    let InitData {
                                        actor,
                                        player_ticket_radius,
                                    } = init_data;

                                    return Ok(SceneSwitch::Game {
                                        parameters: GameSceneParameters {
                                            connection: (tx, rx),
                                            player_actor: actor,
                                            player_ticket_radius,
                                        },
                                    });
                                },
                                Err(message) => {
                                    menu.queue_message(Message::Error(message.to_string()));
                                },
                            }
                        },
                        MainMenuAction::Exit => {
                            return Ok(SceneSwitch::Exit);
                        },
                    }
                },
            }
        }

        Ok(SceneSwitch::Exit)
    }
}
