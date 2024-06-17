use crate::{
    assets::DEFAULT_FONT_PATH,
    system::render::output_thread::OutputThread,
};
use backtrace::Backtrace;
use egui::{
    FontData,
    FontDefinitions,
    FontFamily,
    FontId,
    TextStyle,
};
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

mod assets;
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

    thread::Builder::new().name("runtime".to_owned()).spawn(move || {
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
                            .into_iter()
                            .find(|adapter| {
                                adapter.is_surface_supported(&window_handle.surface)
                                    && adapter.get_info().device_type != wgpu::DeviceType::DiscreteGpu
                            })
                            .expect("no supported GPU adapters present");

                        let (device, queue) = adapter
                            .request_device(
                                &wgpu::DeviceDescriptor {
                                    required_features: wgpu::Features::TEXTURE_BINDING_ARRAY
                                        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
                                    required_limits: wgpu::Limits::default(),
                                    label: None,
                                },
                                None,
                            )
                            .await
                            .unwrap();

                        let physical_size = window_handle.window.inner_size();

                        let capabilities = window_handle
                            .surface
                            .get_capabilities(&adapter);

                        let format = capabilities.formats.iter()
                            .copied()
                            .find(|f| *f == wgpu::TextureFormat::Rgba8UnormSrgb)
                            .expect("The GPU does not support Rgba8UnormSrgb texture format");

                        let config = wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format,
                            width: physical_size.width,
                            height: physical_size.height,
                            present_mode: wgpu::PresentMode::Fifo,
                            desired_maximum_frame_latency: 2,
                            alpha_mode: wgpu::CompositeAlphaMode::Auto,
                            view_formats: vec![format],
                        };

                        let context = egui::Context::default();

                        // TODO defined dynamically in settings
                        context.set_pixels_per_point(1.5);

                        let viewport_id = context.viewport_id();

                        let font = voxbrix_common::read_file_async(DEFAULT_FONT_PATH).await
                            .expect("unable to read default font");

                        let mut fonts = FontDefinitions::default();

                        let mut font = FontData::from_owned(font);

                        font.tweak.y_offset = 4.0;

                        fonts.font_data.insert("default".to_owned(), font);

                        fonts.families.get_mut(&FontFamily::Proportional).unwrap()
                            .insert(0, "default".to_owned());

                        context.set_fonts(fonts);

                        let mut style = (*context.style()).clone();

                        style.text_styles = [
                          (TextStyle::Heading, FontId::new(22.0, FontFamily::Proportional)),
                          (TextStyle::Body, FontId::new(22.0, FontFamily::Proportional)),
                          (TextStyle::Monospace, FontId::new(22.0, FontFamily::Proportional)),
                          (TextStyle::Button, FontId::new(22.0, FontFamily::Proportional)),
                          (TextStyle::Small, FontId::new(22.0, FontFamily::Proportional)),
                        ].into();

                        context.set_style(style);

                        let interface_state = egui_winit::State::new(
                            context,
                            viewport_id,
                            &window_handle.window,
                            None,
                            None,
                        );

                        let output_thread = OutputThread::new(
                            RenderHandle {
                                adapter,
                                device,
                                queue,
                            },
                            window_handle,
                            config,
                            None,
                        );

                        let interface_renderer = egui_wgpu::Renderer::new(
                            &output_thread.device(),
                            output_thread.current_surface_config().format,
                            None,
                            1,
                        );

                        let scene_manager = SceneManager {
                            interface_state,
                            interface_renderer,
                            output_thread,
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
    }).expect("unable to spawn the runtime thread");

    window::create_window(window_tx).expect("window creation");
}
