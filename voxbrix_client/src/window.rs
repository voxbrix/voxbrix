use egui_wgpu::ScreenDescriptor;
use flume::{
    Receiver,
    Sender,
    TrySendError,
};
use log::info;
use std::{
    mem,
    sync::Arc,
    thread,
    time::{
        Duration,
        Instant,
    },
};
pub use winit::event::{
    DeviceEvent,
    WindowEvent,
};
use winit::{
    application::ApplicationHandler,
    event::DeviceId,
    event_loop::{
        ActiveEventLoop,
        EventLoop,
        EventLoopProxy,
    },
    window::{
        CursorGrabMode,
        Fullscreen,
        Window as WinitWindow,
        WindowId,
    },
};

const SURFACE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

enum App {
    Initialized(Initialized),
    Uninitialized(Option<Args>),
}

struct Shared {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

struct Initialized {
    shared: Arc<Shared>,
    window: Arc<WinitWindow>,
    surface: wgpu::Surface<'static>,
    surface_texture: Option<wgpu::SurfaceTexture>,
    surface_config: wgpu::SurfaceConfiguration,
    surface_reconfigure: bool,
    frame_time: Option<Duration>,
    last_render: Instant,
    input_tx: Sender<InputEvent>,
    request_tx: Sender<Frame>,
    ui_state: egui_winit::State,
    cursor_visible: bool,
}

struct Args {
    window_tx: Sender<Window>,
    submit_tx: EventLoopProxy<Frame>,
}

impl ApplicationHandler<Frame> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Self::Uninitialized(args) = self {
            let Args {
                window_tx,
                submit_tx,
            } = args.take().unwrap();

            let attributes = WinitWindow::default_attributes()
                .with_title("Voxbrix")
                .with_fullscreen(Some(Fullscreen::Borderless(None)));

            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .expect("unable to create window"),
            );

            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                ..Default::default()
            });

            let surface = instance
                .create_surface(window.clone())
                .expect("unable to create surface");

            let (input_tx, input_rx) = flume::bounded(32);

            let adapter = instance
                .enumerate_adapters(wgpu::Backends::VULKAN)
                .into_iter()
                .find(|adapter| {
                    adapter.is_surface_supported(&surface)
                        && adapter.get_info().device_type != wgpu::DeviceType::DiscreteGpu
                })
                .expect("no supported GPU adapters present");

            let required_features = wgpu::Features::TEXTURE_BINDING_ARRAY
                | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING;

            let mut required_limits = wgpu::Limits::default();
            required_limits.max_binding_array_elements_per_shader_stage = 500000;

            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    required_features,
                    required_limits,
                    label: None,
                    memory_hints: Default::default(),
                    trace: wgpu::Trace::Off,
                    experimental_features: Default::default(),
                }))
                .expect("unable to get requested device");

            let capabilities = surface.get_capabilities(&adapter);

            let format = capabilities
                .formats
                .iter()
                .copied()
                .find(|f| *f == SURFACE_TEXTURE_FORMAT)
                .unwrap_or_else(|| {
                    panic!(
                        "The GPU does not support {:?} texture format",
                        SURFACE_TEXTURE_FORMAT
                    )
                });

            let surface_size = window.inner_size();

            let surface_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: surface_size.width,
                height: surface_size.height,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![format],
            };

            surface.configure(&device, &surface_config);

            let ui_context = egui::Context::default();

            let mut ui_state = egui_winit::State::new(
                ui_context.clone(),
                ui_context.viewport_id(),
                window.as_ref(),
                None,
                None,
                None,
            );

            let (request_tx, request_rx) = flume::bounded(1);

            let shared = Arc::new(Shared { device, queue });

            let surface_texture = surface
                .get_current_texture()
                .expect("unable to acquire next output texture");

            let cursor_visible = false;
            let _ = window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked));
            window.set_cursor_visible(cursor_visible);

            let renderer =
                egui_wgpu::Renderer::new(&shared.device, surface_config.format, Default::default());

            let _ = request_tx.try_send(Frame {
                encoders: Vec::new(),
                view: surface_texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default()),
                ui_renderer: UiRenderer {
                    shared: shared.clone(),
                    context: ui_context.clone(),
                    renderer,
                    size: surface_texture.texture.size(),
                    io: UiRendererIo::Input(ui_state.take_egui_input(&window)),
                },
                cursor_visible,
            });

            window_tx
                .send(Window {
                    shared: shared.clone(),
                    input_source: input_rx,
                    submit_tx,
                    request_rx,
                    ui_context,
                    texture_format: surface_config.format,
                    cursor_visible,
                })
                .expect("window handle receiver dropped");

            *self = Self::Initialized(Initialized {
                shared,
                window,
                surface,
                surface_texture: Some(surface_texture),
                surface_config,
                surface_reconfigure: true,
                frame_time: None,
                last_render: Instant::now(),
                input_tx,
                request_tx,
                ui_state,
                cursor_visible,
            });
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Self::Initialized(app) = self else {
            return;
        };

        let send = match &event {
            WindowEvent::Resized(size) => {
                app.surface_config.width = size.width;
                app.surface_config.height = size.height;
                app.surface_reconfigure = true;

                false
            },
            WindowEvent::KeyboardInput { .. }
            | WindowEvent::CloseRequested
            | WindowEvent::MouseInput { .. } => true,
            _ => false,
        };

        if app.cursor_visible {
            let _ = app.ui_state.on_window_event(app.window.as_ref(), &event);
        }

        if send {
            match app.input_tx.try_send(InputEvent::WindowEvent(event)) {
                Err(TrySendError::Disconnected(_)) => {
                    info!("event channel closed, exiting window loop");
                    return;
                },
                Err(TrySendError::Full(_)) | Ok(_) => {},
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let Self::Initialized(app) = self else {
            return;
        };

        match app.input_tx.try_send(InputEvent::DeviceEvent(event)) {
            Err(TrySendError::Disconnected(_)) => {
                info!("event channel closed, exiting window loop");
                return;
            },
            Err(TrySendError::Full(_)) | Ok(_) => {},
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Frame) {
        let Self::Initialized(app) = self else {
            return;
        };

        let Frame {
            mut encoders,
            view: _,
            mut ui_renderer,
            cursor_visible,
        } = event;

        app.shared
            .queue
            .submit(encoders.drain(..).map(|enc| enc.finish()));

        app.surface_texture.take().unwrap().present();

        if let UiRendererIo::Output(output) = mem::take(&mut ui_renderer.io) {
            app.ui_state
                .handle_platform_output(app.window.as_ref(), output)
        }

        if app.surface_reconfigure {
            app.surface
                .configure(&app.shared.device, &app.surface_config);
            app.surface_reconfigure = false;
        }

        if app.cursor_visible != cursor_visible {
            app.cursor_visible = cursor_visible;

            if cursor_visible {
                let _ = app.window.set_cursor_grab(CursorGrabMode::None);
            } else {
                let _ = app
                    .window
                    .set_cursor_grab(CursorGrabMode::Confined)
                    .or_else(|_| app.window.set_cursor_grab(CursorGrabMode::Locked));
            }
            app.window.set_cursor_visible(cursor_visible);
        }

        if let Some(frame_time) = app.frame_time {
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(app.last_render);

            if let Some(to_wait) = frame_time.checked_sub(elapsed) {
                app.last_render = app.last_render + frame_time;
                thread::sleep(to_wait);
            } else {
                app.last_render = now;
            }
        }

        ui_renderer.io = UiRendererIo::Input(app.ui_state.take_egui_input(app.window.as_ref()));

        let surface_texture = app
            .surface
            .get_current_texture()
            .expect("unable to acquire next output texture");

        ui_renderer.size = surface_texture.texture.size();

        let send_result = app.request_tx.try_send(Frame {
            encoders,
            view: surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
            ui_renderer,
            cursor_visible: app.cursor_visible,
        });

        app.surface_texture = Some(surface_texture);

        if send_result.is_err() {
            event_loop.exit();
        }
    }
}

enum UiRendererIo {
    Input(egui::RawInput),
    Pending,
    Output(egui::PlatformOutput),
}

impl Default for UiRendererIo {
    fn default() -> Self {
        Self::Pending
    }
}

// Trying to reuse the same context with a different renderer
// causes the rendered textures to become invisible, so we have
// to also reuse the renderer.
// Probably a bug in egui_wgpu::Renderer (?)
pub struct UiRenderer {
    shared: Arc<Shared>,
    context: egui::Context,
    renderer: egui_wgpu::Renderer,
    size: wgpu::Extent3d,
    io: UiRendererIo,
}

impl UiRenderer {
    pub fn context(&self) -> &egui::Context {
        &self.context
    }

    pub fn render_output(
        &mut self,
        output: egui::FullOutput,
        encoder: &mut wgpu::CommandEncoder,
        render_pass_descriptor: &wgpu::RenderPassDescriptor,
    ) {
        self.io = UiRendererIo::Output(output.platform_output);

        let clipped_primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: output.pixels_per_point,
        };

        self.renderer.update_buffers(
            &self.shared.device,
            &self.shared.queue,
            encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        for (id, image_delta) in &output.textures_delta.set {
            self.renderer
                .update_texture(&self.shared.device, &self.shared.queue, *id, image_delta);
        }

        let mut render_pass = encoder
            .begin_render_pass(render_pass_descriptor)
            .forget_lifetime();

        self.renderer
            .render(&mut render_pass, &clipped_primitives, &screen_descriptor);
    }
}

pub struct Window {
    shared: Arc<Shared>,
    input_source: Receiver<InputEvent>,
    submit_tx: EventLoopProxy<Frame>,
    request_rx: Receiver<Frame>,
    ui_context: egui::Context,
    texture_format: wgpu::TextureFormat,
    pub cursor_visible: bool,
}

impl Window {
    pub fn create(window_tx: Sender<Self>) {
        let event_loop = EventLoop::with_user_event()
            .build()
            .expect("unable to build event loop");

        let mut app = App::Uninitialized(Some(Args {
            window_tx,
            submit_tx: event_loop.create_proxy(),
        }));

        event_loop
            .run_app(&mut app)
            .expect("run loop exited with error");
    }

    pub fn get_frame_source(&self) -> Receiver<Frame> {
        self.request_rx.clone()
    }

    pub fn get_input_source(&self) -> Receiver<InputEvent> {
        self.input_source.clone()
    }

    pub fn submit_frame(&self, mut frame: Frame) {
        frame.cursor_visible = self.cursor_visible;
        let _ = self.submit_tx.send_event(frame);
    }

    pub fn ui_context(&self) -> &egui::Context {
        &self.ui_context
    }

    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.texture_format
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.shared.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.shared.queue
    }
}

#[derive(Debug)]
pub enum InputEvent {
    DeviceEvent(DeviceEvent),
    WindowEvent(WindowEvent),
}

pub struct Frame {
    pub encoders: Vec<wgpu::CommandEncoder>,
    pub view: wgpu::TextureView,
    pub ui_renderer: UiRenderer,
    cursor_visible: bool,
}

impl Frame {
    pub fn size(&self) -> wgpu::Extent3d {
        self.ui_renderer.size
    }

    pub fn take_ui_input(&mut self) -> egui::RawInput {
        let UiRendererIo::Input(input) = mem::take(&mut self.ui_renderer.io) else {
            panic!("input already taken");
        };

        input
    }
}
