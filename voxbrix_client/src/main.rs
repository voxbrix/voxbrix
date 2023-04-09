use async_executor::LocalExecutor;
use futures_lite::future;
use log::error;
use scene::MainScene;
use std::{
    panic,
    process,
    thread,
    time::Duration,
};
use window::{
    WindowCommand,
    WindowHandle,
};
use winit::event_loop::EventLoopProxy;

mod camera;
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

    let default_panic = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        default_panic(panic_info);
        process::exit(1);
    }));

    let (window_tx, window_rx) = flume::bounded::<WindowHandle>(1);
    let (event_proxy_tx, event_proxy_rx) = flume::bounded::<EventLoopProxy<WindowCommand>>(1);

    thread::spawn(move || {
        let rt = &Box::leak(Box::new(LocalExecutor::new()));
        future::block_on(rt.run(async move {
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
                                // WebGL doesn't support all of wgpu's features, so if
                                // we're building for the web we'll have to disable some.
                                limits: if cfg!(target_arch = "wasm32") {
                                    wgpu::Limits::downlevel_webgl2_defaults()
                                } else {
                                    wgpu::Limits::default()
                                },
                                label: None,
                            },
                            None, // Trace path
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
                    let main_loop = MainScene {
                        rt,
                        window_handle,
                        render_handle,
                    };
                    if let Err(err) = main_loop.run().await {
                        error!("main_loop ended with error: {:?}", err);
                    }
                },
            }
        }));

        // Cleanup to prevent tasks from being aborted instead of dropped correctly
        // when the main thread exits
        // WARNING: no task should be .detach() to run endlessly
        // prefer to avoid .detach() altogether
        // just have a handle around to be dropped
        // at the end of the scope (that effectively drops the task)
        while !rt.is_empty() {
            rt.try_tick();
        }

        match event_proxy_rx.recv() {
            Ok(tx) => {
                let _ = tx.send_event(WindowCommand::Shutdown);
            },
            Err(_) => {
                error!("unable to receive window proxy to send shutdown");
            },
        }
    });

    if let Err(err) = window::create_window(window_tx, event_proxy_tx) {
        error!("unable to create window: {:?}", err);
    }
}
