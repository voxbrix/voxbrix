use async_executor::LocalExecutor;
use futures_lite::future;
use log::error;
use scene::SceneManager;
use std::{
    panic,
    process,
    rc::Rc,
    thread,
    time::Duration,
};
use window::WindowHandle;

mod component;
mod entity;
mod scene;
mod system;
mod window;

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct RenderHandle {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

fn main() {
    env_logger::init();

    let (window_tx, window_rx) = flume::bounded::<WindowHandle>(1);
    let (panic_tx, panic_rx) = flume::bounded(1);

    thread::spawn(move || {
        let is_panic = panic::catch_unwind(|| {
            let rt = Rc::new(LocalExecutor::new());

            // Route all panics into the async runtime.
            // It allows to properly drop() everything in runtime loops.
            // This is required, for example, to send DISCONNECT even on panic.
            // TODO the same for signals
            rt.spawn(async move {
                if let Ok(_) = panic_rx.recv_async().await {
                    panic::resume_unwind(Box::new(()));
                }
            })
            .detach();

            let default_panic = panic::take_hook();
            let async_thread = thread::current().id();
            panic::set_hook(Box::new(move |panic_info| {
                default_panic(panic_info);
                if thread::current().id() != async_thread {
                    let _ = panic_tx.send(());
                    thread::sleep(Duration::MAX);
                } else {
                    panic::resume_unwind(Box::new(()));
                }
            }));

            future::block_on(rt.clone().run(async move {
                match window_rx.recv_async().await {
                    Err(_) => {
                        error!("unable to receive window handle");
                    },
                    Ok(window_handle) => {
                        let adapter = window_handle.instance
                            .request_adapter(&wgpu::RequestAdapterOptions {
                                power_preference: wgpu::PowerPreference::LowPower,
                                compatible_surface: Some(&window_handle.surface),
                                force_fallback_adapter: false,
                            })
                            .await
                            .unwrap();

                        let (device, queue) = adapter
                            .request_device(
                                &wgpu::DeviceDescriptor {
                                    features: wgpu::Features::TEXTURE_BINDING_ARRAY
                                        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                                    limits: wgpu::Limits::default(),
                                    label: None,
                                },
                                None,
                            )
                            .await
                            .unwrap();

                        let window_handle = Box::leak(Box::new(window_handle));

                        let render_handle = Box::leak(Box::new(RenderHandle {
                            adapter,
                            device,
                            queue,
                        }));
                        let scene_manager = SceneManager {
                            rt,
                            window_handle,
                            render_handle,
                        };
                        if let Err(err) = scene_manager.run().await {
                            error!("main_loop ended with error: {:?}", err);
                        }
                    },
                }
            }))
        }).is_err();

        if is_panic {
            process::exit(1);
        } else {
            process::exit(0);
        }
    });

    window::create_window(window_tx).expect("window creation");
}
