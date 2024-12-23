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
    event_loop::EventLoop,
    window::{
        Fullscreen,
        Window,
    },
};

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

pub struct WindowHandle {
    pub window: &'static Window,
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub event_rx: Receiver<InputEvent>,
}

pub fn create_window(handle_tx: Sender<WindowHandle>) -> Result<()> {
    let event_loop = EventLoop::new()?;

    // Mailbox (Fast Vsync) and Immediate (No Vsync) work best with
    // the current rendering approrientation_ach
    // Vulkan supports Mailbox present mode reliably and is cross-platform
    // https://github.com/gfx-rs/wgpu/issues/2128
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::default(),
        dx12_shader_compiler: wgpu::Dx12Compiler::default(),
        gles_minor_version: wgpu::Gles3MinorVersion::Automatic,
    });

    let attributes = Window::default_attributes()
        .with_title("Voxbrix")
        .with_fullscreen(Some(Fullscreen::Borderless(None)));

    let window: &'static _ = Box::leak(Box::new(event_loop.create_window(attributes)?));

    let surface = instance.create_surface(window).unwrap();

    let (event_tx, event_rx) = flume::bounded(32);

    handle_tx
        .send(WindowHandle {
            window,
            instance,
            surface,
            event_rx,
        })
        .map_err(|_| Error::msg("surface channel is closed"))?;

    event_loop.run(move |event, _| {
        let send = match event {
            Event::DeviceEvent { device_id, event } => {
                Some(InputEvent::DeviceEvent { device_id, event })
            },
            Event::WindowEvent {
                window_id: _,
                event,
            } => Some(InputEvent::WindowEvent { event }),
            _ => None,
        };

        if let Some(event) = send {
            if let Err(_) = event_tx.send(event) {
                info!("event channel closed, exiting window loop");
                return;
            }
        }
    })?;

    Ok(())
}
