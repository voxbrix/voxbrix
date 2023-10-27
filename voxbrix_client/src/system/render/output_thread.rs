use crate::{
    window::{
        InputEvent,
        WindowHandle,
    },
    RenderHandle,
};
use flume::{
    Receiver,
    Sender,
};
use std::{
    sync::Arc,
    thread,
    time::{
        Duration,
        Instant,
    },
};
use winit::window::Window;

pub struct OutputBundle {
    encoders: Vec<wgpu::CommandEncoder>,
    output: wgpu::SurfaceTexture,
    surface_config: wgpu::SurfaceConfiguration,
}

impl OutputBundle {
    pub fn encoders(&mut self) -> &mut Vec<wgpu::CommandEncoder> {
        &mut self.encoders
    }

    pub fn output(&self) -> &wgpu::SurfaceTexture {
        &self.output
    }
}

struct Submission {
    present: OutputBundle,
    frame_time: Option<Duration>,
    surface_config_updated: bool,
}

fn copy_surface_config(to: &mut wgpu::SurfaceConfiguration, from: &wgpu::SurfaceConfiguration) {
    to.view_formats.clear();
    to.view_formats
        .extend_from_slice(from.view_formats.as_slice());

    let wgpu::SurfaceConfiguration {
        usage,
        format,
        width,
        height,
        present_mode,
        alpha_mode,
        view_formats: _,
    } = to;

    *usage = from.usage;
    *format = from.format;
    *width = from.width;
    *height = from.height;
    *present_mode = from.present_mode;
    *alpha_mode = from.alpha_mode;
}

pub struct OutputThread {
    render_handle: Arc<RenderHandle>,
    window: Window,
    instance: wgpu::Instance,
    input_source: Receiver<InputEvent>,
    current_surface_config: wgpu::SurfaceConfiguration,
    next_surface_config: wgpu::SurfaceConfiguration,
    frame_time: Option<Duration>,
    submit_tx: Sender<Submission>,
    request_rx: Receiver<OutputBundle>,
}

impl OutputThread {
    pub fn new(
        render_handle: RenderHandle,
        window_handle: WindowHandle,
        surface_config: wgpu::SurfaceConfiguration,
        frame_time: Option<Duration>,
    ) -> Self {
        let render_handle = Arc::new(render_handle);
        let WindowHandle {
            window,
            instance,
            surface,
            event_rx: input_source,
        } = window_handle;

        let (submit_tx, submit_rx) = flume::bounded::<Submission>(1);
        let (request_tx, request_rx) = flume::bounded::<OutputBundle>(1);

        let current_surface_config = surface_config.clone();
        let next_surface_config = surface_config.clone();

        let render_handle_inner = render_handle.clone();

        thread::Builder::new()
            .name("output".to_owned())
            .spawn(move || {
                let mut last_render = Instant::now();

                surface.configure(&render_handle_inner.device, &surface_config);

                let output = surface
                    .get_current_texture()
                    .expect("unable to acquire next output texture");

                let _ = request_tx.try_send(OutputBundle {
                    encoders: Vec::new(),
                    output,
                    surface_config: surface_config.clone(),
                });

                while let Ok(Submission {
                    present,
                    frame_time,
                    surface_config_updated,
                }) = submit_rx.recv()
                {
                    let OutputBundle {
                        mut encoders,
                        output,
                        surface_config,
                    } = present;

                    render_handle_inner
                        .queue
                        .submit(encoders.drain(..).map(|enc| enc.finish()));

                    output.present();

                    if surface_config_updated {
                        surface.configure(&render_handle_inner.device, &surface_config);
                    }

                    if let Some(frame_time) = frame_time {
                        let now = Instant::now();
                        let elapsed = now.saturating_duration_since(last_render);

                        if let Some(to_wait) = frame_time.checked_sub(elapsed) {
                            last_render = last_render + frame_time;
                            thread::sleep(to_wait);
                        } else {
                            last_render = now;
                        }
                    }

                    let output = surface
                        .get_current_texture()
                        .expect("unable to acquire next output texture");

                    let _ = request_tx.try_send(OutputBundle {
                        encoders,
                        output,
                        surface_config,
                    });
                }
            })
            .expect("unable to spawn the output thread");

        Self {
            render_handle,
            window,
            instance,
            input_source,
            current_surface_config,
            next_surface_config,
            frame_time,
            submit_tx,
            request_rx,
        }
    }

    pub fn present_output(&mut self, mut output: OutputBundle) {
        let surface_config_updated = self.current_surface_config != self.next_surface_config;

        if surface_config_updated {
            copy_surface_config(&mut self.current_surface_config, &self.next_surface_config);
            copy_surface_config(&mut output.surface_config, &self.next_surface_config);
        }

        self.submit_tx
            .try_send(Submission {
                present: output,
                frame_time: self.frame_time,
                surface_config_updated,
            })
            .expect("unable to present output");
    }

    pub fn current_surface_config(&self) -> &wgpu::SurfaceConfiguration {
        &self.current_surface_config
    }

    pub fn next_surface_config(&mut self) -> &mut wgpu::SurfaceConfiguration {
        &mut self.next_surface_config
    }

    pub fn get_surface_source(&self) -> Receiver<OutputBundle> {
        self.request_rx.clone()
    }

    pub fn get_input_source(&self) -> Receiver<InputEvent> {
        self.input_source.clone()
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.render_handle.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.render_handle.queue
    }

    pub fn frame_time(&mut self) -> &mut Option<Duration> {
        &mut self.frame_time
    }
}
