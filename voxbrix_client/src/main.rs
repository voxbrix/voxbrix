use async_executor::LocalExecutor;
use futures_lite::future;
use log::{
    error,
    info,
};
use parking_lot::Mutex;
use std::{
    panic::{
        self,
        PanicInfo,
    },
    thread,
};
use wgpu::{
    Backends,
    Instance,
    Surface,
};
use window::WindowEvent;
use winit::{
    dpi::PhysicalSize,
    event_loop::EventLoopProxy,
};

mod camera;
mod component;
mod entity;
mod event_loop;
mod linear_algebra;
mod system;
mod vertex;
mod window;
mod manager;

#[macro_export]
macro_rules! unblock {
    (($($a:ident),+)$e:expr) => {
        {
            let res;

            (($($a),+), res) = blocking::unblock(move || -> Result<_> {
                let res = $e;
                Ok((($($a),+), res))
            }).await?;

            res
        }
    };
}

fn print_panic_info(panic_info: &PanicInfo<'_>) {
    error!("panic in: {:?}", panic_info.location());

    if let Some(panic_payload) = panic_info.payload().downcast_ref::<&str>() {
        error!("panic with: \"{}\"", panic_payload);
    } else if let Some(panic_payload) = panic_info.payload().downcast_ref::<String>() {
        error!("panic with: \"{}\"", panic_payload);
    }
}

fn main() {
    env_logger::init();
    info!("Starting!");

    panic::set_hook(Box::new(move |panic_info| {
        print_panic_info(panic_info);
    }));

    let (surface_tx, surface_rx) = async_oneshot::oneshot::<(
        Instance,
        Surface,
        PhysicalSize<u32>,
        Mutex<EventLoopProxy<WindowEvent>>,
    )>();
    let (window_event_tx, window_event_rx) = flume::bounded(32);

    let backends = Backends::VULKAN;

    thread::spawn(move || {
        let rt = LocalExecutor::new();
        future::block_on(rt.run(async move {
            match surface_rx.await {
                Err(_) => {
                    error!("unable to receive surface");
                },
                Ok((instance, surface, surface_size, panic_window_tx)) => {
                    let window_tx = panic_window_tx.lock().clone();

                    panic::set_hook(Box::new(move |panic_info| {
                        print_panic_info(panic_info);
                        let _ = panic_window_tx.lock().send_event(WindowEvent::Shutdown);
                    }));

                    if let Err(err) =
                        event_loop::run(instance, surface, surface_size, window_event_rx).await
                    {
                        error!("event loop ended with error: {:?}", err);
                    }

                    let _ = window_tx.send_event(WindowEvent::Shutdown);
                },
            }
        }))
    });

    if let Err(err) = window::create_window(backends, surface_tx, window_event_tx) {
        error!("unable to create window: {:?}", err);
    }
}
