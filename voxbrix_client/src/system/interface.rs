use crate::{
    system::render::Renderer,
    window::Frame,
};
use egui::Context;

pub struct InterfaceSystem {
    context: Option<Context>,
}

impl InterfaceSystem {
    pub fn new() -> Self {
        Self { context: None }
    }

    /// Call this before adding interfaces.
    pub fn start(&mut self, frame: &mut Frame) {
        self.context = Some(frame.ui_renderer.context().clone());

        self.context
            .as_ref()
            .unwrap()
            .begin_pass(frame.take_ui_input());
    }

    pub fn add_interface(&self, interface: impl FnOnce(&Context)) {
        interface(
            self.context
                .as_ref()
                .expect("must start before adding interface"),
        );
    }

    /// Finishes the composition and renders the result.
    pub fn render(&mut self, renderer: Renderer) {
        let interface = self
            .context
            .as_ref()
            .expect("must start interface before rendering")
            .end_pass();

        self.context = None;

        renderer
            .ui_renderer
            .expect("renderer has no UI renderer")
            .render_output(
                interface,
                renderer.encoder,
                &wgpu::RenderPassDescriptor {
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
                },
            );
    }
}
