use crate::{
    system::render::{
        output_thread::OutputThread,
        Renderer,
    },
    InterfaceData,
};
use anyhow::Result;
use egui::Context;
use egui_wgpu::renderer::{
    Renderer as InterfaceRenderer,
    ScreenDescriptor,
};
use winit::{
    event::WindowEvent,
    window::Window,
};

pub struct InterfaceSystemDescriptor<'a> {
    pub interface_data: InterfaceData,
    pub output_thread: &'a OutputThread,
}

impl InterfaceSystemDescriptor<'_> {
    pub fn build(self) -> InterfaceSystem {
        let Self {
            interface_data,
            output_thread,
        } = self;

        InterfaceSystem {
            interface_data,
            interface_renderer: InterfaceRenderer::new(
                &output_thread.device(),
                output_thread.current_surface_config().format,
                None,
                1,
            ),
            context: Context::default(),
        }
    }
}

pub struct InterfaceSystem {
    interface_data: InterfaceData,
    interface_renderer: InterfaceRenderer,
    context: Context,
}

impl InterfaceSystem {
    /// Call this before adding interfaces.
    pub fn start(&mut self, window: &Window) {
        let input = self.interface_data.state.take_egui_input(window);
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

        let clipped_primitives = self.context.tessellate(interface.shapes, 2.0);

        self.interface_renderer.update_buffers(
            renderer.device,
            renderer.queue,
            renderer.encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        for (id, image_delta) in &interface.textures_delta.set {
            self.interface_renderer.update_texture(
                renderer.device,
                renderer.queue,
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
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        self.interface_renderer
            .render(&mut render_pass, &clipped_primitives, &screen_descriptor);

        Ok(())
    }

    pub fn window_event(&mut self, event: &WindowEvent) {
        // TODO only redraw if required
        let _ = self
            .interface_data
            .state
            .on_window_event(&self.context, event);
    }

    pub fn into_interface_data(self) -> InterfaceData {
        self.interface_data
    }
}
