use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            position::PositionActorComponent,
        },
        actor_model::body_part::BodyPartActorModelComponent,
    },
    system::render::{
        vertex::Vertex,
        RenderParameters,
        Renderer,
    },
    RenderHandle,
};
use anyhow::Result;
use voxbrix_common::entity::actor::Actor;
use wgpu::util::DeviceExt;

const INDEX_FORMAT: wgpu::IndexFormat = wgpu::IndexFormat::Uint32;

pub struct ActorRenderSystemDescriptor<'a> {
    pub render_handle: &'static RenderHandle,
    pub render_parameters: RenderParameters<'a>,
    pub actor_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub actor_texture_bind_group: wgpu::BindGroup,
}

impl<'a> ActorRenderSystemDescriptor<'a> {
    pub async fn build(self) -> ActorRenderSystem {
        let Self {
            render_handle,
            render_parameters:
                RenderParameters {
                    camera_bind_group_layout,
                    texture_format,
                },
            actor_texture_bind_group_layout,
            actor_texture_bind_group,
        } = self;

        let shaders = render_handle
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shaders"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders.wgsl").into()),
            });

        let render_pipeline_layout =
            render_handle
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[
                        &camera_bind_group_layout,
                        &actor_texture_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        let render_pipeline =
            render_handle
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(&render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shaders,
                        entry_point: "vs_main",
                        buffers: &[Vertex::desc()],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shaders,
                        entry_point: "fs_main",
                        targets: &[Some(wgpu::ColorTargetState {
                            format: texture_format,
                            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
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
                });

        ActorRenderSystem {
            render_handle,
            render_pipeline,
            actor_texture_bind_group,
            indices: Vec::new(),
            vertices: Vec::new(),
        }
    }
}

pub struct ActorRenderSystem {
    render_handle: &'static RenderHandle,
    render_pipeline: wgpu::RenderPipeline,
    actor_texture_bind_group: wgpu::BindGroup,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl ActorRenderSystem {
    pub fn update(
        &mut self,
        player_actor: Actor,
        class_ac: &ClassActorComponent,
        position_ac: &PositionActorComponent,
        body_part_amc: &BodyPartActorModelComponent,
    ) {
        self.vertices.clear();
        self.indices.clear();

        for (_actor, _class, position) in position_ac
            .iter()
            .filter(|(actor, _)| *actor != player_actor)
            .filter_map(|(actor, position)| Some((actor, class_ac.get(&actor)?, position)))
        {
            for (_, _body_part, body_part_builder) in
                body_part_amc.get_actor_model(crate::entity::actor_model::ActorModel(0))
            {
                body_part_builder.build(position, &mut self.vertices, &mut self.indices);
            }
        }
    }

    pub fn render(&self, renderer: Renderer) -> Result<(), wgpu::SurfaceError> {
        let vertex_size = self.vertices.len();
        let index_size = self.indices.len();

        if vertex_size == 0 || index_size == 0 {
            return Ok(());
        }

        let prepared_vertex_buffer =
            self.render_handle
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("actor_vertex_buffer"),
                    contents: bytemuck::cast_slice(self.vertices.as_slice()),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });

        let prepared_index_buffer =
            self.render_handle
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("actor_index_buffer"),
                    contents: bytemuck::cast_slice(self.indices.as_slice()),
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                });

        let mut render_pass = renderer.with_pipeline(&self.render_pipeline);

        render_pass.set_bind_group(1, &self.actor_texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, prepared_vertex_buffer.slice(..));
        render_pass.set_index_buffer(prepared_index_buffer.slice(..), INDEX_FORMAT);
        render_pass.draw_indexed(0 .. index_size as u32, 0, 0 .. 1);

        Ok(())
    }
}
