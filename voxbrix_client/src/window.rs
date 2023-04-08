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
        DeviceEvent,
        DeviceId,
        Event,
        WindowEvent,
    },
    event_loop::{
        ControlFlow,
        EventLoopBuilder,
        EventLoopProxy,
    },
    window::{
        Fullscreen,
        Window,
        WindowBuilder,
    },
};

#[derive(Debug)]
pub enum InputEvent {
    DeviceEvent {
        device_id: DeviceId,
        event: DeviceEvent,
    },
    WindowEvent {
        event: WindowEvent<'static>,
    },
}

pub enum WindowCommand {
    Shutdown,
}

pub struct WindowHandle {
    pub window: Window,
    pub event_rx: Receiver<InputEvent>,
    // TODO investigate if send_event() is blocking:
    // wayland: std unbound channel, not blocking
    //     https://github.com/rust-windowing/winit/blob/master/src/platform_impl/linux/wayland/event_loop/mod.rs
    //     https://github.com/Smithay/calloop/blob/master/src/sources/channel.rs
    // x11:
    // windows:
    // mac:
    pub event_tx: EventLoopProxy<WindowCommand>,
}

pub fn create_window(
    handle_tx: Sender<WindowHandle>,
    event_proxy_tx: Sender<EventLoopProxy<WindowCommand>>,
) -> Result<()> {
    let event_loop = EventLoopBuilder::with_user_event().build();

    event_proxy_tx
        .send(event_loop.create_proxy())
        .map_err(|_| Error::msg("event proxy channel is closed"))?;

    let window = WindowBuilder::new().build(&event_loop)?;

    window.set_fullscreen(Some(Fullscreen::Borderless(None)));

    let (event_tx, event_rx) = flume::bounded(32);

    handle_tx
        .send(WindowHandle {
            window,
            event_rx,
            event_tx: event_loop.create_proxy(),
        })
        .map_err(|_| Error::msg("surface channel is closed"))?;

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let send = match event {
            Event::DeviceEvent { device_id, event } => {
                Some(InputEvent::DeviceEvent { device_id, event })
            },
            Event::WindowEvent {
                window_id: _,
                event,
            } => {
                event
                    .to_static()
                    .map(|event| InputEvent::WindowEvent { event })
            },
            Event::UserEvent(event) => {
                match event {
                    WindowCommand::Shutdown => {
                        *flow = ControlFlow::Exit;
                    },
                }

                None
            },
            _ => None,
        };

        if let Some(event) = send {
            if let Err(_) = event_tx.send(event) {
                *flow = ControlFlow::Exit;
                info!("event channel closed, exiting window loop");
                return;
            }
        }
    });
}
