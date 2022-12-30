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
    dpi::{
        PhysicalPosition,
        PhysicalSize,
    },
    event::{
        DeviceEvent,
        DeviceId,
        ElementState,
        Event,
        KeyboardInput,
        ModifiersState,
        MouseButton,
        MouseScrollDelta,
        TouchPhase,
        WindowEvent as WinitWindowEvent,
    },
    event_loop::{
        ControlFlow,
        EventLoopBuilder,
        EventLoopProxy,
    },
    window::{
        Window,
        WindowBuilder,
    },
};

// winit's WindowEvent subset, just because we don't want lifetimes
#[derive(Debug)]
pub enum WindowEvent {
    Resized(PhysicalSize<u32>),
    CloseRequested,
    Destroyed,
    KeyboardInput {
        device_id: DeviceId,
        input: KeyboardInput,
        is_synthetic: bool,
    },
    MouseInput {
        device_id: DeviceId,
        state: ElementState,
        button: MouseButton,
    },
    ModifiersChanged(ModifiersState),
    Focused(bool),
    ReceivedCharacter(char),
    CursorMoved {
        device_id: DeviceId,
        position: PhysicalPosition<f64>,
    },
    MouseWheel {
        device_id: DeviceId,
        delta: MouseScrollDelta,
        phase: TouchPhase,
    },
}

impl WindowEvent {
    fn from_winit(from: WinitWindowEvent) -> Result<Self, WinitWindowEvent> {
        match from {
            WinitWindowEvent::KeyboardInput {
                device_id,
                input,
                is_synthetic,
            } => {
                Ok(WindowEvent::KeyboardInput {
                    device_id,
                    input,
                    is_synthetic,
                })
            },
            WinitWindowEvent::MouseInput {
                device_id,
                state,
                button,
                ..
            } => {
                Ok(WindowEvent::MouseInput {
                    device_id,
                    state,
                    button,
                })
            },
            WinitWindowEvent::ModifiersChanged(s) => Ok(WindowEvent::ModifiersChanged(s)),
            WinitWindowEvent::Resized(s) => Ok(WindowEvent::Resized(s)),
            WinitWindowEvent::CloseRequested => Ok(WindowEvent::CloseRequested),
            WinitWindowEvent::Destroyed => Ok(WindowEvent::Destroyed),
            WinitWindowEvent::Focused(b) => Ok(WindowEvent::Focused(b)),
            WinitWindowEvent::ReceivedCharacter(ch) => Ok(WindowEvent::ReceivedCharacter(ch)),
            WinitWindowEvent::CursorMoved {
                device_id,
                position,
                ..
            } => {
                Ok(WindowEvent::CursorMoved {
                    device_id,
                    position,
                })
            },
            WinitWindowEvent::MouseWheel {
                device_id,
                delta,
                phase,
                ..
            } => {
                Ok(WindowEvent::MouseWheel {
                    device_id,
                    delta,
                    phase,
                })
            },
            WinitWindowEvent::ScaleFactorChanged {
                scale_factor: _,
                new_inner_size,
            } => Ok(WindowEvent::Resized(*new_inner_size)),
            _ => Err(from),
        }
    }

    pub fn to_iced(
        &self,
        scale_factor: f64,
        modifiers: ModifiersState,
    ) -> Option<iced_winit::event::Event> {
        use iced_winit::{
            conversion,
            event::Event,
            keyboard,
            mouse,
            window,
            Point,
        };

        match self {
            WindowEvent::Resized(size) => {
                let logical_size = size.to_logical(scale_factor);

                Some(Event::Window(window::Event::Resized {
                    width: logical_size.width,
                    height: logical_size.height,
                }))
            },
            WindowEvent::CloseRequested => Some(Event::Window(window::Event::CloseRequested)),
            WindowEvent::KeyboardInput {
                input:
                    winit::event::KeyboardInput {
                        virtual_keycode: Some(virtual_keycode),
                        state,
                        ..
                    },
                ..
            } => {
                Some(Event::Keyboard({
                    let key_code = conversion::key_code(*virtual_keycode);
                    let modifiers = conversion::modifiers(modifiers);

                    match state {
                        winit::event::ElementState::Pressed => {
                            keyboard::Event::KeyPressed {
                                key_code,
                                modifiers,
                            }
                        },
                        winit::event::ElementState::Released => {
                            keyboard::Event::KeyReleased {
                                key_code,
                                modifiers,
                            }
                        },
                    }
                }))
            },
            WindowEvent::MouseInput { button, state, .. } => {
                let button = conversion::mouse_button(*button);

                Some(Event::Mouse(match state {
                    winit::event::ElementState::Pressed => mouse::Event::ButtonPressed(button),
                    winit::event::ElementState::Released => mouse::Event::ButtonReleased(button),
                }))
            },
            WindowEvent::ModifiersChanged(new_modifiers) => {
                Some(Event::Keyboard(keyboard::Event::ModifiersChanged(
                    conversion::modifiers(*new_modifiers),
                )))
            },
            WindowEvent::Focused(focused) => {
                Some(Event::Window(if *focused {
                    window::Event::Focused
                } else {
                    window::Event::Unfocused
                }))
            },
            WindowEvent::ReceivedCharacter(c) if !is_private_use_character(*c) => {
                Some(Event::Keyboard(keyboard::Event::CharacterReceived(*c)))
            },
            WindowEvent::CursorMoved { position, .. } => {
                let position = position.to_logical::<f64>(scale_factor);

                Some(Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(position.x as f32, position.y as f32),
                }))
            },
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    winit::event::MouseScrollDelta::LineDelta(delta_x, delta_y) => {
                        Some(Event::Mouse(mouse::Event::WheelScrolled {
                            delta: mouse::ScrollDelta::Lines {
                                x: *delta_x,
                                y: *delta_y,
                            },
                        }))
                    },
                    winit::event::MouseScrollDelta::PixelDelta(position) => {
                        Some(Event::Mouse(mouse::Event::WheelScrolled {
                            delta: mouse::ScrollDelta::Pixels {
                                x: position.x as f32,
                                y: position.y as f32,
                            },
                        }))
                    },
                }
            },
            _ => None,
        }
    }
}

pub(crate) fn is_private_use_character(c: char) -> bool {
    matches!(
        c,
        '\u{E000}'..='\u{F8FF}'
        | '\u{F0000}'..='\u{FFFFD}'
        | '\u{100000}'..='\u{10FFFD}'
    )
}

#[derive(Debug)]
pub enum InputEvent {
    DeviceEvent {
        device_id: DeviceId,
        event: DeviceEvent,
    },
    WindowEvent {
        event: WindowEvent,
    },
}

impl InputEvent {
    fn from_winit<T>(from: Event<T>) -> Result<Self, Event<T>> {
        match from {
            Event::DeviceEvent { device_id, event } => {
                Ok(InputEvent::DeviceEvent { device_id, event })
            },
            Event::WindowEvent { window_id, event } => {
                Ok(InputEvent::WindowEvent {
                    event: WindowEvent::from_winit(event)
                        .map_err(|event| Event::WindowEvent { window_id, event })?,
                })
            },
            _ => Err(from),
        }
    }
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

    let (event_tx, event_rx) = flume::bounded(32);

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
        match InputEvent::from_winit(event) {
            Ok(event) => send!(event, flow),
            Err(event) => {
                match event {
                    Event::UserEvent(event) => {
                        match event {
                            WindowCommand::Shutdown => {
                                *flow = ControlFlow::Exit;
                            },
                        }
                    },

                    _ => {},
                }
            },
        }
    });
}
