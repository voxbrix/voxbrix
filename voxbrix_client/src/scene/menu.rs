use crate::{
    scene::{
        game::GameSceneParameters,
        SceneSwitch,
    },
    window::{
        Frame,
        InputEvent,
        Window,
        WindowEvent,
    },
    CONNECTION_TIMEOUT,
};
use anyhow::Result;
use argon2::Argon2;
use egui::CentralPanel;
use futures_lite::{
    future,
    StreamExt as _,
};
use k256::ecdsa::{
    signature::{
        Signer as _,
        Verifier as _,
    },
    Signature,
    SigningKey,
    VerifyingKey,
};
use log::warn;
use serde::Deserialize;
use tokio::{
    task::{
        self,
        JoinHandle,
    },
    time,
};
use voxbrix_common::{
    async_ext::StreamExt as _,
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
    pack::{
        Pack,
        Packer,
    },
};
use voxbrix_protocol::client::{
    Client,
    Connection,
    Receiver,
    Sender,
};

pub struct MenuSceneParameters {
    pub window: Window,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum ActionType {
    Login,
    Registration,
}

enum Event {
    Process(Frame),
    Input(InputEvent),
}

pub struct MenuScene {
    pub parameters: MenuSceneParameters,
}

impl MenuScene {
    pub async fn run(self) -> Result<SceneSwitch> {
        let Self {
            parameters: MenuSceneParameters { mut window },
        } = self;

        window.cursor_visible = true;

        let frame_source = window.get_frame_source();
        let input_source = window.get_input_source();

        let mut stream = frame_source
            .stream()
            .map(Event::Process)
            .or_ff(input_source.stream().map(Event::Input));

        let mut error_message = String::new();

        let mut prev_form = Form {
            server_address: Default::default(),
            username: Default::default(),
            password: Default::default(),
            password_confirmation: Default::default(),
            action: ActionType::Login,
        };

        let mut is_registration = false;

        let mut form = prev_form.clone();

        let mut connect_task: Option<JoinHandle<Result<_, String>>> = None;

        while let Some(event) = stream.next().await {
            if prev_form != form {
                error_message.clear();
                prev_form = form.clone();
            }
            match event {
                Event::Process(mut frame) => {
                    let input = frame.take_ui_input();

                    let full_output = window.ui_context().run(input, |ctx| {
                        CentralPanel::default().show(&ctx, |ui| {
                            ui.label("Voxbrix");
                            ui.label(&error_message);
                            ui.label("Server socket address:");
                            ui.text_edit_singleline(&mut form.server_address);
                            ui.label("Username:");
                            ui.text_edit_singleline(&mut form.username);
                            ui.label("Password:");
                            ui.text_edit_singleline(&mut form.password);
                            if is_registration {
                                ui.label("Password confirmation:");
                                ui.text_edit_singleline(&mut form.password_confirmation);
                            }
                            ui.add_space(16.0);
                            if ui.button("Submit").clicked() && connect_task.is_none() {
                                form.action = match is_registration {
                                    false => ActionType::Login,
                                    true => ActionType::Registration,
                                };

                                let form = form.clone();

                                connect_task = Some(task::spawn_local(async move {
                                    form.connect().await.map_err(|msg| msg.to_owned())
                                }));
                            }
                            ui.add_space(16.0);
                            ui.checkbox(&mut is_registration, "Registration");
                        });
                    });

                    let mut encoder =
                        window
                            .device()
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("UI Render Encoder"),
                            });

                    frame.ui_renderer.render_output(
                        full_output,
                        &mut encoder,
                        &wgpu::RenderPassDescriptor {
                            label: Some("Render Pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &frame.view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.1,
                                        g: 0.1,
                                        b: 0.1,
                                        a: 0.0,
                                    }),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        },
                    );

                    frame.encoders.push(encoder);

                    window.submit_frame(frame);

                    if let Some(ct) = connect_task.as_ref() {
                        if ct.is_finished() {
                            match connect_task.take().unwrap().await.unwrap() {
                                Ok((tx, rx, init_data)) => {
                                    let InitData {
                                        actor,
                                        player_chunk_view_radius,
                                    } = init_data;

                                    return Ok(SceneSwitch::Game {
                                        parameters: GameSceneParameters {
                                            window,
                                            connection: (tx, rx),
                                            player_actor: actor,
                                            player_chunk_view_radius,
                                        },
                                    });
                                },
                                Err(err) => {
                                    error_message = err;
                                },
                            }
                        }
                    }
                },
                Event::Input(event) => {
                    if let InputEvent::WindowEvent(event) = event {
                        match event {
                            WindowEvent::CloseRequested => {
                                return Ok(SceneSwitch::Exit);
                            },
                            _ => {},
                        }
                    }
                },
            }
        }

        Ok(SceneSwitch::Exit)
    }
}

pub async fn send_recv<R>(
    buf: &[u8],
    tx: &mut Sender,
    rx: &mut Receiver,
    packer: &mut Packer,
) -> Result<R, &'static str>
where
    for<'a> R: Pack + Deserialize<'a>,
{
    let (send_res, recv_res) = time::timeout(CONNECTION_TIMEOUT, async {
        future::zip(
            async {
                tx.send_reliable(0, &buf)
                    .await
                    .map_err(|_| "Unable to send initialization request")?;

                tx.wait_complete()
                    .await
                    .map_err(|_| "Unable to send initialization request")
            },
            async {
                loop {
                    let (_channel, bytes) = rx
                        .recv()
                        .await
                        .map_err(|_| "Unable to get initialization response")?;

                    if let Ok(res) = packer.unpack::<R>(bytes) {
                        return Ok(res);
                    } else {
                        warn!("unknown message, skipping");
                    }
                }
            },
        )
        .await
    })
    .await
    .map_err(|_| "Connection timeout")?;

    send_res?;

    recv_res
}

#[derive(Clone, Debug)]
struct Form {
    server_address: String,
    username: String,
    password: String,
    password_confirmation: String,
    action: ActionType,
}

impl PartialEq<Form> for Form {
    fn eq(&self, other: &Self) -> bool {
        self.server_address == other.server_address
            && self.username == other.username
            && self.password == other.password
            && self.password_confirmation == other.password_confirmation
    }
}

impl Eq for Form {}

impl Form {
    pub async fn connect(&self) -> Result<(Sender, Receiver, InitData), &'static str> {
        let mut tx_buffer = Vec::new();
        let mut packer = Packer::new();
        let socket: std::net::SocketAddr = ([0, 0, 0, 0], 0).into();
        let server: std::net::SocketAddr = self
            .server_address
            .parse()
            .map_err(|_| "Incorrect server socket address format")?;

        let Connection {
            self_key,
            peer_key,
            mut sender,
            mut receiver,
        } = time::timeout(CONNECTION_TIMEOUT, async {
            Client::bind(socket)
                .await
                .map_err(|_| "Unable to bind socket")?
                .connect(server)
                .await
                .map_err(|_| "Connection error")
        })
        .await
        .map_err(|_| "Connection timeout")??;

        let tx = &mut sender;
        let rx = &mut receiver;

        match self.action {
            ActionType::Login => packer.pack(&InitRequest::Login, &mut tx_buffer),
            ActionType::Registration => packer.pack(&InitRequest::Register, &mut tx_buffer),
        };

        let InitResponse {
            public_key: server_key,
            key_signature,
        } = send_recv::<InitResponse>(&tx_buffer, tx, rx, &mut packer).await?;

        let server_key = VerifyingKey::from_sec1_bytes(&server_key)
            .map_err(|_| "Server provided incorrect public key")?;

        let key_signature = Signature::from_bytes((&key_signature).into())
            .map_err(|_| "Server provided incorrect signature")?;

        server_key
            .verify(&peer_key, &key_signature)
            .map_err(|_| "Server signature does not match the public key provided")?;

        if let ActionType::Registration = self.action {
            if self.password != self.password_confirmation {
                return Err("Password and self.password confirmation do not match");
            }
        }

        let mut signing_key = [0; 32];
        Argon2::default()
            .hash_password_into(
                self.password.as_bytes(),
                self.username.as_bytes(),
                &mut signing_key,
            )
            .unwrap();

        let signing_key =
            SigningKey::from_bytes((&signing_key).into()).expect("signing key derive");

        let init_data = match self.action {
            ActionType::Login => {
                let signature: Signature = signing_key.sign(&self_key);
                packer.pack(
                    &LoginRequest {
                        username: self.username.clone(),
                        key_signature: signature.to_bytes().into(),
                    },
                    &mut tx_buffer,
                );

                let response = send_recv::<LoginResult>(&tx_buffer, tx, rx, &mut packer).await?;

                match response {
                    LoginResult::Success(data) => data,
                    LoginResult::Failure(_) => {
                        // TODO: display actual error
                        return Err("Incorrect login credentials");
                    },
                }
            },
            ActionType::Registration => {
                packer.pack(
                    &RegisterRequest {
                        username: self.username.clone(),
                        public_key: signing_key
                            .verifying_key()
                            .to_encoded_point(true)
                            .as_bytes()
                            .try_into()
                            .unwrap(),
                    },
                    &mut tx_buffer,
                );

                let response = send_recv::<RegisterResult>(&tx_buffer, tx, rx, &mut packer).await?;

                match response {
                    RegisterResult::Success(data) => data,
                    RegisterResult::Failure(_) => {
                        // TODO: display actual error
                        return Err("Username already taken");
                    },
                }
            },
        };

        Ok((sender, receiver, init_data))
    }
}
