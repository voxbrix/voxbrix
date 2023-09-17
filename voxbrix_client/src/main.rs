use backtrace::Backtrace;
use log::error;
use scene::SceneManager;
use std::{
    fmt,
    panic::{
        self,
        PanicInfo,
    },
    process,
    thread::{
        self,
        Thread,
    },
    time::Duration,
};
use tokio::{
    runtime::Builder as RuntimeBuilder,
    task::{
        self,
        LocalSet,
    },
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

struct PanicLogEntry<'a> {
    panic_info: &'a PanicInfo<'a>,
    thread: &'a Thread,
    backtrace: Backtrace,
}

impl<'a> fmt::Debug for PanicLogEntry<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self.panic_info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => {
                match self.panic_info.payload().downcast_ref::<String>() {
                    Some(s) => &**s,
                    None => "Box<Any>",
                }
            },
        };

        write!(
            fmt,
            "thread '{}' panicked at '{}': ",
            self.thread.name().unwrap_or("<unnamed>"),
            msg
        )?;

        if let Some(location) = self.panic_info.location() {
            write!(fmt, "{}:{}", location.file(), location.line())?;
        }

        if !self.backtrace.frames().is_empty() {
            write!(fmt, "\n{:?}", self.backtrace)?;
        }

        Ok(())
    }
}

fn main() {
    env_logger::init();

    let (window_tx, window_rx) = flume::bounded::<WindowHandle>(1);
    let (panic_tx, panic_rx) = flume::bounded(1);
    let main_thread = thread::current().id();

    thread::spawn(move || {
        let is_panic = panic::catch_unwind(|| {
            let rt = RuntimeBuilder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
                .expect("unable to build runtime");

            rt.block_on(LocalSet::new().run_until(async move {
                // Route all panics into the async runtime.
                // It allows to properly drop() everything in runtime loops.
                // This is required, for example, to send DISCONNECT even on panic.
                // TODO the same for signals
                task::spawn_local(async move {
                    if let Ok(_) = panic_rx.recv_async().await {
                        panic::resume_unwind(Box::new(()));
                    }
                });

                let async_thread = thread::current().id();
                panic::set_hook(Box::new(move |panic_info| {

                    let this_thread = thread::current();

                    let log_entry = PanicLogEntry {
                        panic_info: &panic_info,
                        thread: &this_thread,
                        backtrace: Backtrace::new(),
                    };

                    error!(target: "panic", "{:?}", log_entry);

                    let this_thread = this_thread.id();

                    if this_thread == async_thread {
                        panic::resume_unwind(Box::new(()));
                    } else if this_thread == main_thread {
                        let _ = panic_tx.try_send(());
                        thread::sleep(Duration::MAX);
                    } else {
                        let _ = panic_tx.try_send(());
                        panic::resume_unwind(Box::new(()));
                    }
                }));

                match window_rx.recv_async().await {
                    Err(_) => {
                        error!("unable to receive window handle");
                    },
                    Ok(window_handle) => {
                        let adapter = window_handle
                            .instance
                            .enumerate_adapters(wgpu::Backends::VULKAN)
                            .find(|adapter| {
                                adapter.is_surface_supported(&window_handle.surface)
                                    && adapter.get_info().device_type != wgpu::DeviceType::DiscreteGpu
                            })
                            .expect("no supported GPU adapters present");

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
                            window_handle,
                            render_handle,
                        };
                        if let Err(err) = scene_manager.run().await {
                            error!("main_loop ended with error: {:#}", err);
                        }
                    },
                }
            }));
        }).is_err();

        if is_panic {
            process::exit(1);
        } else {
            process::exit(0);
        }
    });

    window::create_window(window_tx).expect("window creation");
}
