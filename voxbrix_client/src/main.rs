use async_executor::LocalExecutor;
use event_loop::EventLoop;
use futures_lite::future;
use log::{
    error,
    info,
};
use std::{
    panic::{
        self,
        PanicInfo,
    },
    thread,
};
use wgpu::Backends;
use window::WindowEvent;
use winit::event_loop::EventLoopProxy;

mod camera;
mod component;
mod entity;
mod event_loop;
mod linear_algebra;
mod manager;
mod system;
mod vertex;
mod window;

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

    let (window_tx, window_rx) = flume::bounded(1);
    let (event_proxy_tx, event_proxy_rx) = flume::bounded::<EventLoopProxy<WindowEvent>>(1);

    let backends = Backends::VULKAN;

    thread::spawn(move || {
        let result = std::panic::catch_unwind(move || {
            let rt = LocalExecutor::new();
            future::block_on(rt.run(async move {
                match window_rx.recv_async().await {
                    Err(_) => {
                        error!("unable to receive window handle");
                    },
                    Ok(window) => {
                        let event_loop = EventLoop { window };
                        if let Err(err) = event_loop.run().await {
                            error!("event loop ended with error: {:?}", err);
                        }
                    },
                }
            }))
        });

        if let Err(err) = result {
            error!("event loop panicked: {:?}", err);
        }

        match event_proxy_rx.recv() {
            Ok(tx) => {
                let _ = tx.send_event(WindowEvent::Shutdown);
            },
            Err(_) => {
                error!("unable to receive window proxy to send shutdown");
            },
        }
    });

    if let Err(err) = window::create_window(backends, window_tx, event_proxy_tx) {
        error!("unable to create window: {:?}", err);
    }
}
