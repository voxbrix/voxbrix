use crate::{
    component::actor::{
        orientation::OrientationActorComponent,
        position::PositionActorComponent,
    },
    system::texture_loading::GPU_TEXTURE_FORMAT,
    window::WindowHandle,
    RenderHandle,
};
use arrayvec::ArrayVec;
use camera::{
    Camera,
    CameraParameters,
};
use flume::Receiver;
use output_thread::{
    OutputBundle,
    OutputThread,
};
use std::{
    iter,
    mem,
};
use voxbrix_common::entity::actor::Actor;
use winit::dpi::PhysicalSize;

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
    pub render_handle: &'static RenderHandle,
    pub window_handle: &'static WindowHandle,
    pub player_actor: Actor,
    pub camera_parameters: CameraParameters,
    pub position_ac: &'a PositionActorComponent,
    pub orientation_ac: &'a OrientationActorComponent,
}

impl<'a> RenderSystemDescriptor<'a> {
    pub async fn build(self) -> RenderSystem {
        let Self {
            render_handle,
            window_handle,
            player_actor,
            camera_parameters,
            position_ac,
            orientation_ac,
        } = self;

        let capabilities = window_handle
            .surface
            .get_capabilities(&render_handle.adapter);

        let format = capabilities
            .formats
            .into_iter()
            .find(|format| format == &GPU_TEXTURE_FORMAT)
            .expect("texture format found");

        // let present_mode = capabilities
        // .present_modes
        // .into_iter()
        // .find(|pm| *pm == wgpu::PresentMode::Mailbox)
        // .unwrap_or(wgpu::PresentMode::Immediate);
        let present_mode = wgpu::PresentMode::Fifo;

        let surface_size = window_handle.window.inner_size();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: surface_size.width,
            height: surface_size.height,
            // Fifo makes SurfaceTexture::present() block
            // which is bad for current rendering implementation
            present_mode,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![format],
        };

        window_handle
            .surface
            .configure(&render_handle.device, &config);

        let camera = Camera::new(
            &render_handle.device,
            player_actor,
            camera_parameters,
            position_ac,
            orientation_ac,
        );

        let output_thread = OutputThread::new(
            render_handle,
            window_handle,
            config,
            None,
        );

        let depth_texture_size = wgpu::Extent3d {
            width: surface_size.width,
            height: surface_size.height,
            depth_or_array_layers: 1,
        };

        let depth_texture_view =
            build_depth_texture_view(&render_handle.device, depth_texture_size);

        RenderSystem {
            render_handle,
            camera,
            depth_texture_view,
            depth_texture_size,
            output_thread,
            process: None,
        }
    }
}

#[derive(Debug)]
pub struct Renderer<'a> {
    camera_bind_group: &'a wgpu::BindGroup,
    render_pass: wgpu::RenderPass<'a>,
}

impl<'a> Renderer<'a> {
    pub fn with_pipeline(self, pipeline: &'a wgpu::RenderPipeline) -> wgpu::RenderPass<'a> {
        let Self {
            camera_bind_group,
            mut render_pass,
        } = self;

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
    output: wgpu::SurfaceTexture,
    view: wgpu::TextureView,
    encoders: Vec<wgpu::CommandEncoder>,
}

pub struct RenderSystem {
    render_handle: &'static RenderHandle,
    camera: Camera,
    depth_texture_view: wgpu::TextureView,
    depth_texture_size: wgpu::Extent3d,
    output_thread: OutputThread,
    process: Option<RenderProcess>,
}

impl RenderSystem {
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.output_thread.configure_surface(|config| {
                config.width = new_size.width;
                config.height = new_size.height;
            });
            self.camera.resize(new_size.width, new_size.height);
        }
    }

    pub fn get_render_parameters(&self) -> RenderParameters {
        RenderParameters {
            camera_bind_group_layout: self.camera.get_bind_group_layout(),
            texture_format: GPU_TEXTURE_FORMAT,
        }
    }

    pub fn get_surface_stream(&self) -> Receiver<OutputBundle> {
        self.output_thread.get_surface_stream()
    }

    pub fn update(
        &mut self,
        position_ac: &PositionActorComponent,
        orientation_ac: &OrientationActorComponent,
    ) {
        self.camera
            .update(&self.render_handle.queue, position_ac, orientation_ac);
    }

    pub fn start_render(&mut self, bundle: OutputBundle) {
        let view = bundle
            .output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let view_size = bundle.output.texture.size();

        if view_size != self.depth_texture_size {
            self.depth_texture_size = view_size;
            self.depth_texture_view =
                build_depth_texture_view(&self.render_handle.device, view_size);
        }

        self.process = Some(RenderProcess {
            output: bundle.output,
            encoders: bundle.encoders,
            view,
        });
    }

    /// Returned renderer requires that the camera uniform buffer
    /// has binding group index 0 in the corresponding shaders
    pub fn get_renderers<'a, const N: usize>(&'a mut self) -> [Renderer<'a>; N] {
        let process = self
            .process
            .as_mut()
            .expect("render process must be started");

        let slice_start = process.encoders.len();
        let mut is_first_pass = process.encoders.is_empty();

        let encoders_extend = iter::repeat_with(|| {
            self.render_handle
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                })
        })
        .take(N);

        process.encoders.extend(encoders_extend);

        process.encoders[slice_start ..]
            .iter_mut()
            .map(|encoder| {
                let is_first_pass = mem::replace(&mut is_first_pass, false);

                let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &process.view,
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
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_texture_view,
                        depth_ops: Some(wgpu::Operations {
                            load: if is_first_pass {
                                wgpu::LoadOp::Clear(1.0)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: true,
                        }),
                        stencil_ops: None,
                    }),
                });

                Renderer {
                    camera_bind_group: self.camera.get_bind_group(),
                    render_pass,
                }
            })
            .collect::<ArrayVec<_, N>>()
            .into_inner()
            .unwrap()
    }

    pub fn finish_render(&mut self) {
        let RenderProcess {
            output,
            view: _,
            encoders,
        } = self.process.take().expect("render process must be started");

        self.output_thread
            .present_output(OutputBundle { encoders, output });
    }
}
