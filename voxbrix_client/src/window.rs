use crate::event_loop::Event;
use anyhow::{
    Error,
    Result,
};
use async_oneshot::Sender as OneshotSender;
use flume::Sender;
use log::info;
use parking_lot::Mutex;
use wgpu::{
    Backends,
    Instance,
    Surface,
};
use winit::{
    dpi::PhysicalSize,
    event::{
        DeviceEvent as WinitDeviceEvent,
        Event as WinitEvent,
        WindowEvent as WinitWindowEvent,
    },
    event_loop::{
        ControlFlow,
        EventLoop,
        EventLoopProxy,
    },
    window::WindowBuilder,
};

pub enum WindowEvent {
    Shutdown,
}

pub fn create_window(
    backends: Backends,
    mut surface_tx: OneshotSender<(
        Instance,
        Surface,
        PhysicalSize<u32>,
        Mutex<EventLoopProxy<WindowEvent>>,
    )>,
    event_tx: Sender<Event>,
) -> Result<()> {
    let event_loop = EventLoop::with_user_event();
    let window = WindowBuilder::new().build(&event_loop)?;

    let instance = Instance::new(backends);
    let surface = unsafe { instance.create_surface(&window) };
    surface_tx
        .send((
            instance,
            surface,
            window.inner_size(),
            Mutex::new(event_loop.create_proxy()),
        ))
        .map_err(|_| Error::msg("surface channel is closed"))?;

    window.set_cursor_grab(true)?;
    window.set_cursor_visible(false);

    macro_rules! send {
        ($e:expr, $c:ident) => {
            match event_tx.send($e) {
                Err(_) => {
                    *$c = ControlFlow::Exit;
                    info!("event channel closed, exiting window loop");
                    return;
                },
                _ => {},
            }
        };
    }

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            WinitEvent::DeviceEvent {
                event: WinitDeviceEvent::MouseMotion { delta: (h, v) },
                ..
            } => {
                send!(
                    Event::MouseMove {
                        horizontal: h as f32,
                        vertical: v as f32
                    },
                    flow
                );
            },

            WinitEvent::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() => {
                match event {
                    &WinitWindowEvent::KeyboardInput { input, .. } => {
                        send!(Event::Key { input }, flow);
                    },

                    WinitWindowEvent::CloseRequested => {
                        send!(Event::Shutdown, flow);
                    },

                    WinitWindowEvent::Resized(size) => {
                        send!(Event::WindowResize { new_size: *size }, flow);
                    },

                    WinitWindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        send!(
                            Event::WindowResize {
                                new_size: **new_inner_size
                            },
                            flow
                        );
                    },

                    _ => {},
                }
            },

            WinitEvent::UserEvent(event) => {
                match event {
                    WindowEvent::Shutdown => {
                        *flow = ControlFlow::Exit;
                    },
                }
            },

            _ => {},
        }
    });
}
