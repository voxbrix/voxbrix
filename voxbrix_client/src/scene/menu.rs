use crate::{
    scene::{
        game::GameSceneParameters,
        SceneSwitch,
    },
    window::{
        InputEvent,
        WindowHandle,
    },
    RenderHandle,
    CONNECTION_TIMEOUT,
};
use anyhow::Result;
use argon2::Argon2;
use egui::{
    CentralPanel,
    Context,
};
use egui_wgpu::renderer::{
    Renderer,
    ScreenDescriptor,
};
use egui_winit::State;
use futures_lite::{
    future,
    stream,
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
use std::{
    iter,
    time::Duration,
};
use tokio::{
    task::{
        self,
        JoinHandle,
    },
    time::{
        self,
        MissedTickBehavior,
    },
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
    pack::Pack,
};
use voxbrix_protocol::client::{
    Client,
    Connection,
    Receiver,
    Sender,
};
use winit::event::WindowEvent;

#[derive(Clone, PartialEq, Eq, Debug)]
enum ActionType {
    Login,
    Registration,
}

enum Event {
    Process,
    Input(InputEvent),
}

fn set_ui_scale(scale: f32, sd: &mut ScreenDescriptor, ctx: &Context, state: &mut State) {
    sd.pixels_per_point = scale;
    ctx.set_pixels_per_point(scale);
    state.set_pixels_per_point(scale);
}

pub struct MenuScene {
    pub window_handle: &'static WindowHandle,
    pub render_handle: &'static RenderHandle,
}

impl MenuScene {
    pub async fn run(self) -> Result<SceneSwitch> {
        let physical_size = self.window_handle.window.inner_size();

        let capabilities = self
            .window_handle
            .surface
            .get_capabilities(&self.render_handle.adapter);

        let format = capabilities.formats[0];

        let present_mode = capabilities
            .present_modes
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
            view_formats: vec![format],
        };

        self.window_handle
            .surface
            .configure(&self.render_handle.device, &config);

        let _ = self
            .window_handle
            .window
            .set_cursor_grab(winit::window::CursorGrabMode::None);
        self.window_handle.window.set_cursor_visible(true);

        let mut resized = false;

        let mut screen_descriptor = ScreenDescriptor {
            size_in_pixels: [physical_size.width, physical_size.height],
            pixels_per_point: 1.0,
        };

        let mut renderer = Renderer::new(&self.render_handle.device, format, None, 1);

        let ctx = Context::default();

        let mut state = State::new_with_wayland_display(None);

        set_ui_scale(2.0, &mut screen_descriptor, &ctx, &mut state);

        let mut send_status_interval = time::interval(Duration::from_millis(15));
        send_status_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut stream = stream::poll_fn(|cx| {
            send_status_interval
                .poll_tick(cx)
                .map(|_| Some(Event::Process))
        })
        .or_ff(self.window_handle.event_rx.stream().map(Event::Input));

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

        let mut connect_task: Option<JoinHandle<Result<SceneSwitch, String>>> = None;

        while let Some(event) = stream.next().await {
            if prev_form != form {
                error_message.clear();
                prev_form = form.clone();
            }
            match event {
                Event::Process => {
                    if let Some(ct) = connect_task.as_ref() {
                        if ct.is_finished() {
                            match connect_task.take().unwrap().await.unwrap() {
                                Ok(scene_switch) => {
                                    return Ok(scene_switch);
                                },
                                Err(err) => {
                                    error_message = err;
                                },
                            }
                        }
                    }

                    let input = state.take_egui_input(&self.window_handle.window);
                    let full_output = ctx.run(input, |ctx| {
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
                                    form.connect()
                                        .await
                                        .map(|(tx, rx, init_data)| {
                                            let InitData {
                                                actor,
                                                player_ticket_radius,
                                            } = init_data;

                                            SceneSwitch::Game {
                                                parameters: GameSceneParameters {
                                                    connection: (tx, rx),
                                                    player_actor: actor,
                                                    player_ticket_radius,
                                                },
                                            }
                                        })
                                        .map_err(|msg| msg.to_owned())
                                }));
                            }
                            ui.add_space(16.0);
                            ui.checkbox(&mut is_registration, "Registration");
                        });
                    });
                    let clipped_primitives = ctx.tessellate(full_output.shapes);

                    if resized {
                        let physical_size = self.window_handle.window.inner_size();

                        screen_descriptor.size_in_pixels =
                            [physical_size.width, physical_size.height];

                        let config = wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format,
                            width: physical_size.width,
                            height: physical_size.height,
                            // Fifo makes SurfaceTexture::present() block
                            // which is bad for current rendering implementation
                            present_mode,
                            alpha_mode: wgpu::CompositeAlphaMode::Auto,
                            view_formats: vec![format],
                        };

                        self.window_handle
                            .surface
                            .configure(&self.render_handle.device, &config);

                        resized = false;
                    }

                    let output = self.window_handle.surface.get_current_texture()?;

                    let view = output
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());

                    let mut encoder = self.render_handle.device.create_command_encoder(
                        &wgpu::CommandEncoderDescriptor {
                            label: Some("Render Encoder"),
                        },
                    );

                    renderer.update_buffers(
                        &self.render_handle.device,
                        &self.render_handle.queue,
                        &mut encoder,
                        &clipped_primitives,
                        &screen_descriptor,
                    );

                    for (id, image_delta) in &full_output.textures_delta.set {
                        renderer.update_texture(
                            &self.render_handle.device,
                            &self.render_handle.queue,
                            *id,
                            image_delta,
                        );
                    }

                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.1,
                                    g: 0.1,
                                    b: 0.1,
                                    a: 0.0,
                                }),
                                store: true,
                            },
                        })],
                        depth_stencil_attachment: None,
                    });

                    renderer.render(&mut render_pass, &clipped_primitives, &screen_descriptor);

                    drop(render_pass);

                    self.render_handle
                        .queue
                        .submit(iter::once(encoder.finish()));

                    output.present();
                },
                Event::Input(event) => {
                    if let InputEvent::WindowEvent { event } = event {
                        match event {
                            WindowEvent::Resized(_size) => {
                                resized = true;
                            },
                            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                                return Ok(SceneSwitch::Exit);
                            },
                            _ => {
                                let _ = state.on_event(&ctx, &event);
                            },
                        }
                    }
                },
            }
        }

        Ok(SceneSwitch::Exit)
    }
}

pub async fn send_recv<R>(buf: &[u8], tx: &mut Sender, rx: &mut Receiver) -> Result<R, &'static str>
where
    R: Pack,
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

                    if let Ok(res) = R::unpack(bytes) {
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
            ActionType::Login => InitRequest::Login.pack(&mut tx_buffer),
            ActionType::Registration => InitRequest::Register.pack(&mut tx_buffer),
        };

        let InitResponse {
            public_key: server_key,
            key_signature,
        } = send_recv::<InitResponse>(&tx_buffer, tx, rx).await?;

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
                LoginRequest {
                    username: self.username.clone(),
                    key_signature: signature.to_bytes().into(),
                }
                .pack(&mut tx_buffer);

                let response = send_recv::<LoginResult>(&tx_buffer, tx, rx).await?;

                match response {
                    LoginResult::Success(data) => data,
                    LoginResult::Failure(_) => {
                        // TODO: display actual error
                        return Err("Incorrect login credentials");
                    },
                }
            },
            ActionType::Registration => {
                RegisterRequest {
                    username: self.username.clone(),
                    public_key: signing_key
                        .verifying_key()
                        .to_encoded_point(true)
                        .as_bytes()
                        .try_into()
                        .unwrap(),
                }
                .pack(&mut tx_buffer);

                let response = send_recv::<RegisterResult>(&tx_buffer, tx, rx).await?;

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
