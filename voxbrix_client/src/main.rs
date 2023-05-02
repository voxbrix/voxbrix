use async_executor::LocalExecutor;
use futures_lite::future;
use log::error;
use scene::SceneManager;
use std::{
    panic,
    rc::Rc,
    thread,
    time::Duration,
};
use window::{
    WindowCommand,
    WindowHandle,
};
use winit::event_loop::EventLoopProxy;

mod component;
mod entity;
mod scene;
mod system;
mod window;

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct RenderHandle {
    pub surface: wgpu::Surface,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

fn main() {
    env_logger::init();

    let (window_tx, window_rx) = flume::bounded::<WindowHandle>(1);
    let (event_proxy_tx, event_proxy_rx) = flume::bounded::<EventLoopProxy<WindowCommand>>(1);

    let runtime_handle = thread::spawn(move || {
        let rt = Rc::new(LocalExecutor::new());
        let (panic_tx, panic_rx) = flume::bounded(1);

        // With catch_unwind later it allows to properly
        // drop() everything in async runtime.
        // This is required, for example, to send DISCONNECT even on panic.
        // TODO the same for signals
        let default_panic = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            default_panic(panic_info);
            let _ = panic_tx.send(());
            panic::resume_unwind(Box::new(()));
        }));
        rt.spawn(async move {
            if let Ok(_) = panic_rx.recv_async().await {
                panic::resume_unwind(Box::new(()));
            }
        })
        .detach();

        let _ = panic::catch_unwind(|| {
            future::block_on(rt.clone().run(async move {
                match window_rx.recv_async().await {
                    Err(_) => {
                        error!("unable to receive window handle");
                    },
                    Ok(window_handle) => {
                        // Mailbox (Fast Vsync) and Immediate (No Vsync) work best with
                        // the current rendering approrientation_ach
                        // Vulkan supports Mailbox present mode reliably and is cross-platform
                        // https://github.com/gfx-rs/wgpu/issues/2128
                        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                            backends: wgpu::Backends::VULKAN,
                            dx12_shader_compiler: wgpu::Dx12Compiler::default(),
                        });

                        let surface = unsafe { instance.create_surface(&window_handle.window) }.unwrap();

                        let adapter = instance
                            .request_adapter(&wgpu::RequestAdapterOptions {
                                power_preference: wgpu::PowerPreference::LowPower,
                                compatible_surface: Some(&surface),
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
                            surface,
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
        });

        match event_proxy_rx.recv() {
            Ok(tx) => {
                let _ = tx.send_event(WindowCommand::Shutdown);
            },
            Err(_) => {
                error!("unable to receive window proxy to send shutdown");
            },
        }
    });

    if let Ok(Err(err)) = panic::catch_unwind(|| window::create_window(window_tx, event_proxy_tx)) {
        error!("unable to create window: {:?}", err);
    } else {
        let _ = runtime_handle.join();
    }
}
