use crate::{
    component::actor::{
        orientation::OrientationActorComponent,
        position::PositionActorComponent,
    },
    window::{
        Frame,
        UiRenderer,
        Window,
    },
};
use arrayvec::ArrayVec;
use camera::{
    Camera,
    CameraParameters,
};
use std::{
    iter,
    mem,
};
use voxbrix_common::entity::actor::Actor;

pub mod camera;
pub mod gpu_vec;
pub mod primitives;

fn build_depth_texture_view(device: &wgpu::Device, mut size: wgpu::Extent3d) -> wgpu::TextureView {
    size.depth_or_array_layers = 1;

    let desc = wgpu::TextureDescriptor {
        label: Some("depth_texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[wgpu::TextureFormat::Depth32Float],
    };
    let texture = device.create_texture(&desc);

    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

pub struct RenderSystemDescriptor<'a> {
    pub player_actor: Actor,
    pub camera_parameters: CameraParameters,
    pub position_ac: &'a PositionActorComponent,
    pub orientation_ac: &'a OrientationActorComponent,
    pub window: Window,
}

impl<'a> RenderSystemDescriptor<'a> {
    pub fn build(self) -> RenderSystem {
        let Self {
            player_actor,
            camera_parameters,
            position_ac,
            orientation_ac,
            window,
        } = self;

        let camera = Camera::new(
            &window.device(),
            player_actor,
            camera_parameters,
            position_ac,
            orientation_ac,
        );

        let depth_texture_size = wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        };

        let depth_texture_view = build_depth_texture_view(&window.device(), depth_texture_size);

        RenderSystem {
            camera,
            texture_format: window.texture_format(),
            depth_texture_view,
            depth_texture_size,
            window,
            frame: None,
        }
    }
}

pub struct Renderer<'a> {
    is_first_pass: bool,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub view: &'a wgpu::TextureView,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    /// Only the last renderer will have this present:
    pub ui_renderer: Option<&'a mut UiRenderer>,
    depth_texture_view: &'a wgpu::TextureView,
    camera_bind_group: &'a wgpu::BindGroup,
}

impl<'a> Renderer<'a> {
    pub fn with_pipeline(self, pipeline: &'a wgpu::RenderPipeline) -> wgpu::RenderPass<'a> {
        let Self {
            is_first_pass,
            encoder,
            view,
            device: _,
            queue: _,
            ui_renderer: _,
            depth_texture_view,
            camera_bind_group,
        } = self;

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if is_first_pass {
                        wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.6,
                            b: 0.7,
                            a: 0.0,
                        })
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    load: if is_first_pass {
                        wgpu::LoadOp::Clear(1.0)
                    } else {
                        wgpu::LoadOp::Load
                    },
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, camera_bind_group, &[]);

        render_pass
    }
}

#[derive(Clone, Copy)]
pub struct RenderParameters<'a> {
    pub camera_bind_group_layout: &'a wgpu::BindGroupLayout,
    pub texture_format: wgpu::TextureFormat,
}

pub struct RenderSystem {
    camera: Camera,
    texture_format: wgpu::TextureFormat,
    depth_texture_view: wgpu::TextureView,
    depth_texture_size: wgpu::Extent3d,
    window: Window,
    frame: Option<Frame>,
}

impl RenderSystem {
    pub fn get_render_parameters(&self) -> RenderParameters {
        RenderParameters {
            camera_bind_group_layout: self.camera.get_bind_group_layout(),
            texture_format: self.texture_format,
        }
    }

    pub fn update(
        &mut self,
        position_ac: &PositionActorComponent,
        orientation_ac: &OrientationActorComponent,
    ) {
        self.camera
            .update(self.window.queue(), position_ac, orientation_ac);
    }

    pub fn start_render(&mut self, frame: Frame) {
        let view_size = frame.size();
        self.camera.resize(view_size.width, view_size.height);

        if view_size != self.depth_texture_size {
            self.depth_texture_size = view_size;
            self.depth_texture_view = build_depth_texture_view(self.window.device(), view_size);
        }

        self.frame = Some(frame);
    }

    /// Returned renderer requires that the camera uniform buffer
    /// has binding group index 0 in the corresponding shaders
    pub fn get_renderers<'a, const N: usize>(&'a mut self) -> [Renderer<'a>; N] {
        let device = self.window.device();
        let queue = self.window.queue();

        let frame = self.frame.as_mut().expect("render process must be started");

        let encoders = &mut frame.encoders;

        let slice_start = encoders.len();
        let mut is_first_pass = encoders.is_empty();

        let encoders_extend = iter::repeat_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            })
        })
        .take(N);

        encoders.extend(encoders_extend);

        let mut output = encoders[slice_start ..]
            .iter_mut()
            .map(|encoder| {
                Renderer {
                    is_first_pass: mem::replace(&mut is_first_pass, false),
                    encoder,
                    view: &frame.view,
                    device,
                    queue,
                    ui_renderer: None,
                    depth_texture_view: &self.depth_texture_view,
                    camera_bind_group: &self.camera.get_bind_group(),
                }
            })
            .collect::<ArrayVec<_, N>>()
            .into_inner()
            .unwrap_or_else(|_| unreachable!());

        output.last_mut().unwrap().ui_renderer = Some(&mut frame.ui_renderer);

        output
    }

    pub fn finish_render(&mut self) {
        self.window
            .submit_frame(self.frame.take().expect("render process must be started"));
    }

    pub fn into_window(self) -> Window {
        self.window
    }

    pub fn cursor_visibility(&mut self, visible: bool) {
        self.window.cursor_visible = visible;
    }

    pub fn window(&self) -> &Window {
        &self.window
    }
}
