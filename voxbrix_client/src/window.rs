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
use winit::{
    event::{
        DeviceEvent as WinitDeviceEvent,
        ElementState as WinitElementState,
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
        Window,
        WindowBuilder,
    },
};

pub enum WindowEvent {
    Shutdown,
}

pub struct WindowHandle {
    pub window: Window,
    pub event_rx: Receiver<Event>,
    // TODO investigate if send_event() is blocking:
    // wayland: std unbound channel, not blocking
    //     https://github.com/rust-windowing/winit/blob/master/src/platform_impl/linux/wayland/event_loop/mod.rs
    //     https://github.com/Smithay/calloop/blob/master/src/sources/channel.rs
    // x11:
    // windows:
    // mac:
    pub event_tx: EventLoopProxy<WindowEvent>,
}

pub fn create_window(
    handle_tx: Sender<WindowHandle>,
    event_proxy_tx: Sender<EventLoopProxy<WindowEvent>>,
) -> Result<()> {
    let event_loop = EventLoopBuilder::with_user_event().build();

    event_proxy_tx
        .send(event_loop.create_proxy())
        .map_err(|_| Error::msg("event proxy channel is closed"))?;

    let window = WindowBuilder::new().build(&event_loop)?;

    let (event_tx, event_rx) = flume::bounded(32);

    window
        .set_cursor_grab(CursorGrabMode::Confined)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))?;
    window.set_cursor_visible(false);

    handle_tx
        .send(WindowHandle {
            window,
            event_rx,
            event_tx: event_loop.create_proxy(),
        })
        .map_err(|_| Error::msg("surface channel is closed"))?;

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
                window_id: _,
            } => {
                match event {
                    &WinitWindowEvent::MouseInput { state, button, .. } => {
                        if state == WinitElementState::Pressed {
                            send!(Event::MouseButton { input: button }, flow);
                        }
                    },
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
