use crate::{
    window::WindowHandle,
    RenderHandle,
};
use flume::{
    Receiver,
    Sender,
};
use parking_lot::Mutex;
use std::{
    sync::Arc,
    thread,
};

struct SurfaceConfig {
    config: wgpu::SurfaceConfiguration,
    needs_reconfig: bool,
}

struct Shared {
    new_output: Mutex<Option<wgpu::SurfaceTexture>>,
    surface_config: Mutex<SurfaceConfig>,
}

pub struct OutputThread {
    shared: Arc<Shared>,
    output_tx: Sender<Option<wgpu::SurfaceTexture>>,
    done_rx: Receiver<()>,
}

impl OutputThread {
    pub fn new(
        render_handle: &'static RenderHandle,
        window_handle: &'static WindowHandle,
        config: wgpu::SurfaceConfiguration,
    ) -> Self {
        let (output_tx, output_rx) = flume::bounded::<Option<wgpu::SurfaceTexture>>(1);
        let (done_tx, done_rx) = flume::bounded::<()>(1);
        let shared = Arc::new(Shared {
            new_output: Mutex::new(None),
            surface_config: Mutex::new(SurfaceConfig {
                config,
                needs_reconfig: false,
            }),
        });

        let shared_ref = shared.clone();

        thread::spawn(move || {
            while let Ok(output) = output_rx.recv() {
                if let Some(output) = output {
                    output.present();
                }

                {
                    let mut surface_config = shared_ref.surface_config.lock();

                    if surface_config.needs_reconfig {
                        window_handle
                            .surface
                            .configure(&render_handle.device, &surface_config.config);

                        surface_config.needs_reconfig = false;
                    }
                }

                let new_output = window_handle
                    .surface
                    .get_current_texture()
                    .expect("unable to acquire next output texture");
                let mut new_output_ref = shared_ref.new_output.lock();
                *new_output_ref = Some(new_output);
                let _ = done_tx.try_send(());
            }
        });

        output_tx.try_send(None).expect("unable to request output");

        Self {
            shared,
            output_tx,
            done_rx,
        }
    }

    pub fn take_output(&self) -> Option<wgpu::SurfaceTexture> {
        self.shared.new_output.lock().take()
    }

    pub fn present_output(&self, output: wgpu::SurfaceTexture) {
        self.output_tx
            .try_send(Some(output))
            .expect("unable to present output");
    }

    pub fn get_readiness_stream(&self) -> Receiver<()> {
        self.done_rx.clone()
    }

    pub fn configure_surface<F>(&self, mut config: F)
    where
        F: FnMut(&mut wgpu::SurfaceConfiguration),
    {
        let mut surface_config = self.shared.surface_config.lock();

        config(&mut surface_config.config);

        surface_config.needs_reconfig = true;
    }
}
