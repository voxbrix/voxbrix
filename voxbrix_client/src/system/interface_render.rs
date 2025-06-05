use crate::resource::{
    interface::Interface,
    render_pool::Renderer,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct InterfaceRenderSystem;

impl System for InterfaceRenderSystem {
    type Data<'a> = InterfaceRenderSystemData<'a>;
}

#[derive(SystemData)]
pub struct InterfaceRenderSystemData<'a> {
    interface: &'a mut Interface,
}

impl InterfaceRenderSystemData<'_> {
    pub fn run(&mut self, renderer: Renderer) {
        let interface = self.interface.finalize().end_pass();

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
