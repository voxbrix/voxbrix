use crate::component::actor::{
    orientation::OrientationActorComponent,
    position::PositionActorComponent,
};
use arrayvec::ArrayVec;
use camera::{
    Camera,
    CameraParameters,
};
use log::warn;
use output_thread::{
    OutputBundle,
    OutputThread,
};
use std::{
    iter,
    mem,
};
use voxbrix_common::entity::actor::Actor;
use winit::{
    dpi::PhysicalSize,
    window::CursorGrabMode,
};

pub mod camera;
pub mod gpu_vec;
pub mod output_thread;
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
    pub output_thread: OutputThread,
}

impl<'a> RenderSystemDescriptor<'a> {
    pub fn build(self) -> RenderSystem {
        let Self {
            player_actor,
            camera_parameters,
            position_ac,
            orientation_ac,
            output_thread,
        } = self;

        let sc = output_thread.current_surface_config();

        let camera = Camera::new(
            &output_thread.device(),
            player_actor,
            camera_parameters,
            position_ac,
            orientation_ac,
        );

        let depth_texture_size = wgpu::Extent3d {
            width: sc.width,
            height: sc.height,
            depth_or_array_layers: 1,
        };

        let depth_texture_view =
            build_depth_texture_view(&output_thread.device(), depth_texture_size);

        RenderSystem {
            camera,
            texture_format: sc.format,
            depth_texture_view,
            depth_texture_size,
            output_thread,
            process: None,
        }
    }
}

#[derive(Debug)]
pub struct Renderer<'a> {
    is_first_pass: bool,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub view: &'a wgpu::TextureView,
    pub surface_config: &'a wgpu::SurfaceConfiguration,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    depth_texture_view: &'a wgpu::TextureView,
    camera_bind_group: &'a wgpu::BindGroup,
}

impl<'a> Renderer<'a> {
    pub fn with_pipeline(self, pipeline: &'a wgpu::RenderPipeline) -> wgpu::RenderPass<'a> {
        let Self {
            is_first_pass,
            encoder,
            view,
            surface_config: _,
            device: _,
            queue: _,
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

struct RenderProcess {
    bundle: OutputBundle,
    view: wgpu::TextureView,
}

pub struct RenderSystem {
    camera: Camera,
    texture_format: wgpu::TextureFormat,
    depth_texture_view: wgpu::TextureView,
    depth_texture_size: wgpu::Extent3d,
    output_thread: OutputThread,
    process: Option<RenderProcess>,
}

impl RenderSystem {
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            let config = self.output_thread.next_surface_config();
            config.width = new_size.width;
            config.height = new_size.height;
        }
    }

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
            .update(&self.output_thread.queue(), position_ac, orientation_ac);
    }

    pub fn start_render(&mut self, bundle: OutputBundle) {
        let config = self.output_thread.current_surface_config();
        self.camera.resize(config.width, config.height);

        let view = bundle
            .output()
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let view_size = bundle.output().texture.size();

        if view_size != self.depth_texture_size {
            self.depth_texture_size = view_size;
            self.depth_texture_view =
                build_depth_texture_view(&self.output_thread.device(), view_size);
        }

        self.process = Some(RenderProcess { bundle, view });
    }

    /// Returned renderer requires that the camera uniform buffer
    /// has binding group index 0 in the corresponding shaders
    pub fn get_renderers<'a, const N: usize>(&'a mut self) -> [Renderer<'a>; N] {
        let device = self.output_thread.device();
        let queue = self.output_thread.queue();

        let process = self
            .process
            .as_mut()
            .expect("render process must be started");

        let encoders = process.bundle.encoders();

        let slice_start = encoders.len();
        let mut is_first_pass = encoders.is_empty();

        let encoders_extend = iter::repeat_with(|| {
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            })
        })
        .take(N);

        encoders.extend(encoders_extend);

        encoders[slice_start ..]
            .iter_mut()
            .map(|encoder| {
                Renderer {
                    is_first_pass: mem::replace(&mut is_first_pass, false),
                    encoder,
                    view: &process.view,
                    surface_config: self.output_thread.current_surface_config(),
                    device,
                    queue,
                    depth_texture_view: &self.depth_texture_view,
                    camera_bind_group: &self.camera.get_bind_group(),
                }
            })
            .collect::<ArrayVec<_, N>>()
            .into_inner()
            .unwrap()
    }

    pub fn finish_render(&mut self) {
        let RenderProcess { bundle, view: _ } =
            self.process.take().expect("render process must be started");

        self.output_thread.present_output(bundle);
    }

    pub fn into_output(self) -> OutputThread {
        self.output_thread
    }

    pub fn cursor_visibility(&self, visible: bool) {
        let result = if visible {
            self.output_thread.window().set_cursor_visible(true);
            self.output_thread
                .window()
                .set_cursor_grab(CursorGrabMode::None)
        } else {
            self.output_thread.window().set_cursor_visible(false);
            self.output_thread
                .window()
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| {
                    self.output_thread
                        .window()
                        .set_cursor_grab(CursorGrabMode::Locked)
                })
        };

        if let Err(err) = result {
            warn!("unable to set cursor grab: {:?}", err);
        }
    }

    pub fn output_thread(&self) -> &OutputThread {
        &self.output_thread
    }
}
