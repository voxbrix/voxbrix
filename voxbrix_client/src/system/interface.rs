use crate::{
    system::render::{
        output_thread::OutputThread,
        Renderer,
    },
    RenderHandle,
    WindowHandle,
};
use anyhow::Result;
use egui::Context;
use egui_wgpu::renderer::{
    Renderer as InterfaceRenderer,
    ScreenDescriptor,
};
use winit::event::WindowEvent;

pub struct InterfaceSystemDescriptor<'a> {
    pub render_handle: &'static RenderHandle,
    pub window_handle: &'static WindowHandle,
    pub state: egui_winit::State,
    pub output_thread: &'a OutputThread,
}

impl InterfaceSystemDescriptor<'_> {
    pub fn build(self) -> InterfaceSystem {
        let Self {
            render_handle,
            window_handle,
            state,
            output_thread,
        } = self;

        InterfaceSystem {
            render_handle,
            window_handle,
            state,
            interface_renderer: InterfaceRenderer::new(
                &render_handle.device,
                output_thread.current_surface_config().format,
                None,
                1,
            ),
            context: Context::default(),
        }
    }
}

pub struct InterfaceSystem {
    render_handle: &'static RenderHandle,
    window_handle: &'static WindowHandle,
    state: egui_winit::State,
    interface_renderer: InterfaceRenderer,
    context: Context,
}

impl InterfaceSystem {
    /// Call this before adding interfaces.
    pub fn start(&mut self) {
        let input = self.state.take_egui_input(&self.window_handle.window);
        self.context.begin_frame(input);
    }

    pub fn add_interface(&self, interface: impl FnOnce(&Context)) {
        interface(&self.context);
    }

    /// Finishes the composition and renders the result.
    pub fn render(&mut self, renderer: Renderer) -> Result<(), wgpu::SurfaceError> {
        let interface = self.context.end_frame();

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [
                renderer.surface_config.width,
                renderer.surface_config.height,
            ],
            pixels_per_point: 2.0,
        };

        self.context.set_pixels_per_point(2.0);
        self.state.set_pixels_per_point(2.0);

        let clipped_primitives = self.context.tessellate(interface.shapes);

        self.interface_renderer.update_buffers(
            &self.render_handle.device,
            &self.render_handle.queue,
            renderer.encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        for (id, image_delta) in &interface.textures_delta.set {
            self.interface_renderer.update_texture(
                &self.render_handle.device,
                &self.render_handle.queue,
                *id,
                image_delta,
            );
        }

        let mut render_pass = renderer
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: renderer.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });

        self.interface_renderer
            .render(&mut render_pass, &clipped_primitives, &screen_descriptor);

        Ok(())
    }

    pub fn window_event(&mut self, event: &WindowEvent) {
        // TODO only redraw if required
        let _ = self.state.on_event(&self.context, event);
    }

    pub fn into_interface_state(self) -> egui_winit::State {
        self.state
    }
}
