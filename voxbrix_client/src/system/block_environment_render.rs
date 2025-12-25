use crate::{
    assets::{
        BLOCK_ENVIRONMENT_OVERLAY_SHADER_PATH,
        BLOCK_ENVIRONMENT_SHADER_PATH,
    },
    component::chunk::render_data::EnvRenderDataChunkComponent,
    resource::render_pool::{
        new_quad_index_buffer,
        primitives::Vertex,
        RenderParameters,
        Renderer,
        INDEX_FORMAT,
        INDEX_FORMAT_BYTE_SIZE,
        INITIAL_INDEX_BUFFER_LENGTH,
    },
    window::Window,
};
use voxbrix_world::{
    System,
    SystemData,
};
use wgpu::util::DeviceExt;

pub struct BlockEnvironmentRenderSystemDescriptor<'a> {
    pub render_parameters: RenderParameters<'a>,
    pub block_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub block_texture_bind_group: wgpu::BindGroup,
}

impl<'a> BlockEnvironmentRenderSystemDescriptor<'a> {
    pub async fn build(self, window: &Window) -> BlockEnvironmentRenderSystem {
        let Self {
            render_parameters:
                RenderParameters {
                    camera_bind_group_layout,
                    texture_format,
                },
            block_texture_bind_group_layout,
            block_texture_bind_group,
        } = self;

        let shader = voxbrix_common::read_file_async(BLOCK_ENVIRONMENT_SHADER_PATH)
            .await
            .expect("unable to read shader file");

        let shader =
            std::str::from_utf8(&shader).expect("unable to convert binary file to UTF-8 string");

        let shader = window
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("environment_shader"),
                source: wgpu::ShaderSource::Wgsl(shader.into()),
            });

        let render_pipeline_layout =
            window
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("environment_render_pipeline_layout"),
                    bind_group_layouts: &[
                        &camera_bind_group_layout,
                        &block_texture_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        let render_pipeline =
            window
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("environment_render_pipeline"),
                    layout: Some(&render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[Vertex::desc()],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba8Uint,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: Some(wgpu::Face::Back),
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: true,
                        depth_compare: wgpu::CompareFunction::Less,
                        stencil: wgpu::StencilState::default(),
                        bias: wgpu::DepthBiasState::default(),
                    }),
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                    cache: None,
                });

        let shader = voxbrix_common::read_file_async(BLOCK_ENVIRONMENT_OVERLAY_SHADER_PATH)
            .await
            .expect("unable to read shader file");

        let shader =
            std::str::from_utf8(&shader).expect("unable to convert binary file to UTF-8 string");

        let shader = window
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("overlay_shader"),
                source: wgpu::ShaderSource::Wgsl(shader.into()),
            });

        let overlay_texture_bind_group_layout =
            window
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("overlay_texture_bind_group_layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Uint,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    }],
                });

        let render_pipeline_layout =
            window
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("overlay_render_pipeline_layout"),
                    bind_group_layouts: &[&overlay_texture_bind_group_layout],
                    push_constant_ranges: &[],
                });

        let render_overlay_pipeline =
            window
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("overlay_render_overlay_pipeline"),
                    layout: Some(&render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<u32>() as wgpu::BufferAddress,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Uint32],
                        }],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: texture_format,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Cw,
                        cull_mode: Some(wgpu::Face::Back),
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                    cache: None,
                });

        let index_buffer =
            new_quad_index_buffer(window.device(), window.queue(), INITIAL_INDEX_BUFFER_LENGTH);

        let overlay_vertex_buffer =
            window
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("overlay_vertex_buffer"),
                    contents: bytemuck::bytes_of(&[0u32, 1u32, 2u32]),
                    usage: wgpu::BufferUsages::VERTEX,
                });

        BlockEnvironmentRenderSystem {
            render_pipeline,
            render_overlay_pipeline,
            index_buffer,
            block_texture_bind_group,
            overlay_texture_bind_group_layout,
            overlay_vertex_buffer,
            cache: None,
        }
    }
}

struct CachedData {
    overlay_view: wgpu::TextureView,
    overlay_texture_bind_group: wgpu::BindGroup,
}

pub struct BlockEnvironmentRenderSystem {
    render_pipeline: wgpu::RenderPipeline,
    render_overlay_pipeline: wgpu::RenderPipeline,
    index_buffer: wgpu::Buffer,
    block_texture_bind_group: wgpu::BindGroup,
    overlay_texture_bind_group_layout: wgpu::BindGroupLayout,
    overlay_vertex_buffer: wgpu::Buffer,
    cache: Option<CachedData>,
}

impl System for BlockEnvironmentRenderSystem {
    type Data<'a> = BlockEnvironmentRenderSystemData<'a>;
}

#[derive(SystemData)]
pub struct BlockEnvironmentRenderSystemData<'a> {
    system: &'a mut BlockEnvironmentRenderSystem,
    render_data_cc: &'a mut EnvRenderDataChunkComponent,
}

impl BlockEnvironmentRenderSystemData<'_> {
    pub fn run(&mut self, renderer: Renderer) {
        // Must be one of the last in fact:
        assert!(!renderer.is_first_pass);

        if self.system.cache.as_ref().is_none_or(|cache| {
            renderer.view.texture().size() != cache.overlay_view.texture().size()
        }) {
            let env_texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("overlay_view"),
                size: renderer.view.texture().size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: renderer.view.texture().dimension(),
                format: wgpu::TextureFormat::Rgba8Uint,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[wgpu::TextureFormat::Rgba8Uint],
            });

            let overlay_view = env_texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                ..Default::default()
            });

            let overlay_texture_bind_group =
                renderer
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("texture_bind_group"),
                        layout: &self.system.overlay_texture_bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&overlay_view),
                        }],
                    });

            self.system.cache = Some(CachedData {
                overlay_view,
                overlay_texture_bind_group,
            });
        }

        let max_vertices = self.render_data_cc.prepare_render(&renderer);

        let quad_num = max_vertices / 4;

        let required_index_len = const { INDEX_FORMAT_BYTE_SIZE as u64 * 6 } * quad_num as u64;

        if self.system.index_buffer.size() < required_index_len {
            let size = required_index_len.max(self.system.index_buffer.size() * 2);

            self.system.index_buffer = new_quad_index_buffer(renderer.device, renderer.queue, size);
        }

        let buffers_to_render = self
            .render_data_cc
            .get_visible_buffers(|cp, co, r| renderer.camera.is_object_visible(cp, co, r));

        let mut render_pass = renderer
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("environment_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.system.cache.as_ref().unwrap().overlay_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &renderer.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        render_pass.set_pipeline(&self.system.render_pipeline);
        render_pass.set_bind_group(0, renderer.camera.get_bind_group(), &[]);

        render_pass.set_bind_group(1, &self.system.block_texture_bind_group, &[]);

        for vertex_buffer in buffers_to_render {
            render_pass.set_vertex_buffer(0, vertex_buffer.get_slice());
            let num_indices = vertex_buffer.num_vertices() / 4 * 6;
            render_pass.set_index_buffer(
                self.system
                    .index_buffer
                    .slice(.. num_indices as u64 * const { INDEX_FORMAT_BYTE_SIZE as u64 }),
                INDEX_FORMAT,
            );
            render_pass.draw_indexed(0 .. num_indices, 0, 0 .. 1);
        }

        drop(render_pass);

        // Rendering overlay:
        let mut render_pass = renderer
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("environment_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &renderer.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

        render_pass.set_pipeline(&self.system.render_overlay_pipeline);

        render_pass.set_bind_group(
            0,
            &self
                .system
                .cache
                .as_ref()
                .unwrap()
                .overlay_texture_bind_group,
            &[],
        );

        render_pass.set_vertex_buffer(0, self.system.overlay_vertex_buffer.slice(..));

        render_pass.draw(0 .. 3, 0 .. 1);
    }
}
