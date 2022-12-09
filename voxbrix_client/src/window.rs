use crate::event_loop::Event;
use anyhow::{
    Error,
    Result,
};
use flume::{
    Receiver,
    Sender,
};
use log::info;
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
        EventLoopBuilder,
        EventLoopProxy,
    },
    window::{
        CursorGrabMode,
        WindowBuilder,
    },
};

pub enum WindowEvent {
    Shutdown,
}

pub struct WindowHandle {
    pub instance: Instance,
    pub surface: Surface,
    pub size: PhysicalSize<u32>,
    pub event_rx: Receiver<Event>,
    // event_tx: Sender<WindowEvent>,
    //  this should be implemented in a separate "proxy" thread
    //  that will pull events out of async channel and send them
    //  via send_event()
    //  EventLoopProxy should not be directly passed to
    //  the main event loop, because send_event() method
    //  may be blocking
}

pub fn create_window(
    backends: Backends,
    handle_tx: Sender<WindowHandle>,
    event_proxy_tx: Sender<EventLoopProxy<WindowEvent>>,
) -> Result<()> {
    let event_loop = EventLoopBuilder::with_user_event().build();

    event_proxy_tx
        .send(event_loop.create_proxy())
        .map_err(|_| Error::msg("event proxy channel is closed"))?;

    let window = WindowBuilder::new().build(&event_loop)?;

    let instance = Instance::new(backends);
    let surface = unsafe { instance.create_surface(&window) };

    let (event_tx, event_rx) = flume::bounded(32);

    handle_tx
        .send(WindowHandle {
            instance,
            surface,
            size: window.inner_size(),
            event_rx,
        })
        .map_err(|_| Error::msg("surface channel is closed"))?;

    window
        .set_cursor_grab(CursorGrabMode::Confined)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))?;
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
