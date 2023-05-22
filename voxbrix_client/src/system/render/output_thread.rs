use crate::{
    window::WindowHandle,
    RenderHandle,
};
use bitflags::bitflags;
use flume::{
    Receiver,
    Sender,
};
use parking_lot::Mutex;
use std::{
    sync::Arc,
    thread,
    time::{
        Duration,
        Instant,
    },
};

bitflags! {
    #[derive(Clone, Copy)]
    pub struct Actions: u8 {
        const UPDATE_SURFACE_CONFIG = 0b00000001;
        const UPDATE_TIME_PER_FRAME = 0b00000010;
    }
}

pub struct OutputBundle {
    pub encoders: Vec<wgpu::CommandEncoder>,
    pub output: wgpu::SurfaceTexture,
}

struct Submission {
    present: OutputBundle,
    actions: Actions,
}

struct Shared {
    surface_config: wgpu::SurfaceConfiguration,
    frame_time: Option<Duration>,
}

pub struct OutputThread {
    shared: Arc<Mutex<Shared>>,
    actions: Actions,
    submit_tx: Sender<Submission>,
    request_rx: Receiver<OutputBundle>,
}

impl OutputThread {
    pub fn new(
        render_handle: &'static RenderHandle,
        window_handle: &'static WindowHandle,
        surface_config: wgpu::SurfaceConfiguration,
        mut frame_time: Option<Duration>,
    ) -> Self {
        let (submit_tx, submit_rx) = flume::bounded::<Submission>(1);
        let (request_tx, request_rx) = flume::bounded::<OutputBundle>(1);
        let shared = Arc::new(Mutex::new(Shared {
            surface_config,
            frame_time,
        }));

        let shared_ref = shared.clone();

        thread::spawn(move || {
            let mut last_render = Instant::now();

            window_handle
                .surface
                .configure(&render_handle.device, &shared_ref.lock().surface_config);

            let output = window_handle
                .surface
                .get_current_texture()
                .expect("unable to acquire next output texture");

            let _ = request_tx.try_send(OutputBundle {
                encoders: Vec::new(),
                output,
            });

            while let Ok(Submission { present, actions }) = submit_rx.recv() {
                let OutputBundle {
                    mut encoders,
                    output,
                } = present;

                render_handle
                    .queue
                    .submit(encoders.drain(..).map(|enc| enc.finish()));

                output.present();

                if !actions.is_empty() {
                    let shared = shared_ref.lock();

                    if actions.contains(Actions::UPDATE_SURFACE_CONFIG) {
                        window_handle
                            .surface
                            .configure(&render_handle.device, &shared.surface_config);
                    }

                    if actions.contains(Actions::UPDATE_TIME_PER_FRAME) {
                        frame_time = shared.frame_time;
                    }
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

                let output = window_handle
                    .surface
                    .get_current_texture()
                    .expect("unable to acquire next output texture");

                let _ = request_tx.try_send(OutputBundle { encoders, output });
            }
        });

        Self {
            shared,
            actions: Actions::empty(),
            submit_tx,
            request_rx,
        }
    }

    pub fn present_output(&mut self, output: OutputBundle) {
        self.submit_tx
            .try_send(Submission {
                present: output,
                actions: self.actions,
            })
            .expect("unable to present output");

        self.actions = Actions::empty();
    }

    pub fn get_surface_stream(&self) -> Receiver<OutputBundle> {
        self.request_rx.clone()
    }

    pub fn configure_surface<F>(&mut self, mut config: F)
    where
        F: FnMut(&mut wgpu::SurfaceConfiguration),
    {
        config(&mut self.shared.lock().surface_config);

        self.actions.insert(Actions::UPDATE_SURFACE_CONFIG);
    }

    pub fn set_frame_time(&mut self, t: Option<Duration>) {
        self.shared.lock().frame_time = t;

        self.actions.insert(Actions::UPDATE_TIME_PER_FRAME);
    }
}
