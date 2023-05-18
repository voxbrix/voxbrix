use crate::{
    window::WindowHandle,
    RenderHandle,
};
use bitmask::bitmask;
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

bitmask! {
    pub mask Actions: u8 where flags Action {
        UpdateSurfaceConfig = 0b00000001,
        UpdateTimePerFrame = 0b00000010,
    }
}

struct Submission {
    present: wgpu::SurfaceTexture,
    actions: Actions,
}

struct Shared {
    surface_config: wgpu::SurfaceConfiguration,
    time_per_frame: Option<Duration>,
}

pub struct OutputThread {
    shared: Arc<Mutex<Shared>>,
    actions: Actions,
    submit_tx: Sender<Submission>,
    request_rx: Receiver<wgpu::SurfaceTexture>,
}

impl OutputThread {
    pub fn new(
        render_handle: &'static RenderHandle,
        window_handle: &'static WindowHandle,
        surface_config: wgpu::SurfaceConfiguration,
        mut time_per_frame: Option<Duration>,
    ) -> Self {
        let (submit_tx, submit_rx) = flume::bounded::<Submission>(1);
        let (request_tx, request_rx) = flume::bounded::<wgpu::SurfaceTexture>(1);
        let shared = Arc::new(Mutex::new(Shared {
            surface_config,
            time_per_frame,
        }));

        let shared_ref = shared.clone();

        thread::spawn(move || {
            let mut last_render = Instant::now();

            window_handle
                .surface
                .configure(&render_handle.device, &shared_ref.lock().surface_config);

            let new_output = window_handle
                .surface
                .get_current_texture()
                .expect("unable to acquire next output texture");

            let _ = request_tx.try_send(new_output);

            while let Ok(Submission { present, actions }) = submit_rx.recv() {
                present.present();

                if !actions.is_none() {
                    let shared = shared_ref.lock();

                    if actions.contains(Action::UpdateSurfaceConfig) {
                        window_handle
                            .surface
                            .configure(&render_handle.device, &shared.surface_config);
                    }

                    if actions.contains(Action::UpdateTimePerFrame) {
                        time_per_frame = shared.time_per_frame;
                    }
                }

                if let Some(time_per_frame) = time_per_frame {
                    let now = Instant::now();
                    let elapsed = now.saturating_duration_since(last_render);

                    if let Some(to_wait) = time_per_frame.checked_sub(elapsed) {
                        last_render = last_render + time_per_frame;
                        thread::sleep(to_wait);
                    } else {
                        last_render = now;
                    }
                }

                let new_output = window_handle
                    .surface
                    .get_current_texture()
                    .expect("unable to acquire next output texture");

                let _ = request_tx.try_send(new_output);
            }
        });

        Self {
            shared,
            actions: Actions::none(),
            submit_tx,
            request_rx,
        }
    }

    pub fn present_output(&mut self, output: wgpu::SurfaceTexture) {
        self.submit_tx
            .try_send(Submission {
                present: output,
                actions: self.actions,
            })
            .expect("unable to present output");

        self.actions = Actions::none();
    }

    pub fn get_surface_stream(&self) -> Receiver<wgpu::SurfaceTexture> {
        self.request_rx.clone()
    }

    pub fn configure_surface<F>(&mut self, mut config: F)
    where
        F: FnMut(&mut wgpu::SurfaceConfiguration),
    {
        config(&mut self.shared.lock().surface_config);

        self.actions.set(Action::UpdateSurfaceConfig);
    }

    pub fn set_time_per_frame(&mut self, t: Option<Duration>) {
        self.shared.lock().time_per_frame = t;

        self.actions.set(Action::UpdateTimePerFrame);
    }
}
