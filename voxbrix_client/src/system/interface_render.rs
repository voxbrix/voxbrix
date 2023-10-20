use crate::{
    system::render::{
        output_thread::OutputThread,
        primitives::Polygon,
        Renderer,
    },
    RenderHandle,
    WindowHandle,
};
use anyhow::Result;
use egui::{
    CentralPanel,
    Context,
};
use egui_wgpu::renderer::{
    Renderer as InterfaceRenderer,
    ScreenDescriptor,
};
use winit::event::WindowEvent;

pub struct InterfaceRenderSystemDescriptor<'a> {
    pub render_handle: &'static RenderHandle,
    pub window_handle: &'static WindowHandle,
    pub state: egui_winit::State,
    pub output_thread: &'a OutputThread,
}

impl InterfaceRenderSystemDescriptor<'_> {
    pub fn build(self) -> InterfaceRenderSystem {
        let Self {
            render_handle,
            window_handle,
            state,
            output_thread,
        } = self;

        InterfaceRenderSystem {
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
            inventory_open: false,
        }
    }
}

pub struct InterfaceRenderSystem {
    render_handle: &'static RenderHandle,
    window_handle: &'static WindowHandle,
    state: egui_winit::State,
    interface_renderer: InterfaceRenderer,
    context: Context,
    pub inventory_open: bool,
}

impl InterfaceRenderSystem {
    pub fn render(&mut self, renderer: Renderer) -> Result<(), wgpu::SurfaceError> {
        let input = self.state.take_egui_input(&self.window_handle.window);
        let interface = self.context.run(input, |ctx| {
            egui::Window::new("Inventory")
                .open(&mut self.inventory_open)
                .show(ctx, |ui| {
                    ui.label("Hello World!");
                });
        });

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

    pub fn into_interface_state(self) -> egui_winit::State {
        self.state
    }

    pub fn window_event(&mut self, event: &WindowEvent) {
        // TODO only redraw if required
        let _ = self.state.on_event(&self.context, event);
    }
}
