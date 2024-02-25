use crate::system::render::Renderer;
use anyhow::Result;
use egui::Context;
use egui_wgpu::ScreenDescriptor;
use winit::{
    event::WindowEvent,
    window::Window,
};

pub struct InterfaceSystemDescriptor {
    pub interface_state: egui_winit::State,
    pub interface_renderer: egui_wgpu::Renderer,
}

impl InterfaceSystemDescriptor {
    pub fn build(self) -> InterfaceSystem {
        let Self {
            interface_state,
            interface_renderer,
        } = self;

        InterfaceSystem {
            interface_state,
            interface_renderer,
        }
    }
}

pub struct InterfaceSystem {
    interface_state: egui_winit::State,
    interface_renderer: egui_wgpu::Renderer,
}

impl InterfaceSystem {
    /// Call this before adding interfaces.
    pub fn start(&mut self, window: &Window) {
        let input = self.interface_state.take_egui_input(window);
        self.interface_state.egui_ctx().begin_frame(input);
    }

    pub fn add_interface(&self, interface: impl FnOnce(&Context)) {
        interface(self.interface_state.egui_ctx());
    }

    /// Finishes the composition and renders the result.
    pub fn render(&mut self, renderer: Renderer) -> Result<(), wgpu::SurfaceError> {
        let interface = self.interface_state.egui_ctx().end_frame();

        let pixels_per_point = self.interface_state.egui_ctx().pixels_per_point();

        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [
                renderer.surface_config.width,
                renderer.surface_config.height,
            ],
            pixels_per_point,
        };

        let clipped_primitives = self
            .interface_state
            .egui_ctx()
            .tessellate(interface.shapes, pixels_per_point);

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

    pub fn window_event(&mut self, window: &Window, event: &WindowEvent) {
        // TODO only redraw if required
        let _ = self.interface_state.on_window_event(window, event);
    }

    pub fn destruct(self) -> (egui_winit::State, egui_wgpu::Renderer) {
        (self.interface_state, self.interface_renderer)
    }
}
