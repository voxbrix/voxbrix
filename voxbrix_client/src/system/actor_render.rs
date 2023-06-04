use crate::{
    component::{
        actor::{
            animation_state::{
                AnimationState,
                AnimationStateActorComponent,
            },
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
        },
        actor_model::{
            animation::AnimationActorModelComponent,
            body_part::{
                BodyPartActorModelComponent,
                BASE_BODY_PART,
            },
        },
    },
    entity::actor_model::ActorBodyPart,
    system::render::{
        vertex::Vertex,
        RenderParameters,
        Renderer,
    },
    RenderHandle,
};
use anyhow::Result;
use nohash_hasher::IntMap;
use std::time::Instant;
use voxbrix_common::{
    entity::actor::Actor,
    math::{
        Directions,
        Mat4F32,
        QuatF32,
        Vec3F32,
    },
};
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
            // TODO use nohash?
            body_part_buffer: IntMap::default(),
            indices: Vec::new(),
            vertices: Vec::new(),
        }
    }
}

struct BodyPartInfo {
    transform: Mat4F32,
    parent: ActorBodyPart,
}

pub struct ActorRenderSystem {
    render_handle: &'static RenderHandle,
    render_pipeline: wgpu::RenderPipeline,
    actor_texture_bind_group: wgpu::BindGroup,
    body_part_buffer: IntMap<ActorBodyPart, BodyPartInfo>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl ActorRenderSystem {
    pub fn update(
        &mut self,
        player_actor: Actor,
        class_ac: &ClassActorComponent,
        position_ac: &PositionActorComponent,
        velocity_ac: &VelocityActorComponent,
        orientation_ac: &OrientationActorComponent,
        body_part_amc: &BodyPartActorModelComponent,
        animation_amc: &AnimationActorModelComponent,
        animation_state_ac: &mut AnimationStateActorComponent,
    ) {
        self.vertices.clear();
        self.indices.clear();

        for (actor, _class, position) in position_ac
            .iter()
            .filter(|(actor, _)| *actor != player_actor)
            .filter_map(|(actor, position)| Some((actor, class_ac.get(&actor)?, position)))
        {
            self.body_part_buffer.clear();

            let actor_model = crate::entity::actor_model::ActorModel(0);

            // Orientation
            // TODO swimming / wallclimbing / etc.
            let base_transform = if let Some(body_orient) =
                orientation_ac.get(&actor).and_then(|ori| {
                    let mut direction = ori.forward();
                    direction.z = 0.0;
                    let direction = direction.normalize();
                    if direction.is_nan() {
                        return None;
                    }

                    Some(QuatF32::from_rotation_arc(Vec3F32::FORWARD, direction))
                }) {
                Mat4F32::from_quat(body_orient)
            } else {
                Mat4F32::IDENTITY
            };

            // Walking animation
            // TODO better walking detection
            let walking_animation = crate::entity::actor_model::ActorAnimation(0);
            let walking_animation_duration_ms = 500;
            if velocity_ac
                .get(&actor)
                .filter(|vel| vel.vector.length() > f32::EPSILON)
                .is_some()
            {
                if let Some(anim_builder) = animation_amc.get(actor_model, walking_animation) {
                    let state = match animation_state_ac.get(actor, walking_animation) {
                        Some(s) => s,
                        None => {
                            // TODO have common Instant::now()
                            animation_state_ac.insert(
                                actor,
                                walking_animation,
                                AnimationState {
                                    start: Instant::now(),
                                },
                            );
                            animation_state_ac.get(actor, walking_animation).unwrap()
                        },
                    };

                    let state = (state.start.elapsed().as_millis() % walking_animation_duration_ms)
                        as f32
                        / walking_animation_duration_ms as f32;

                    for (_, body_part, body_part_builder) in
                        body_part_amc.get_actor_model(crate::entity::actor_model::ActorModel(0))
                    {
                        if let Some(new_transform) = anim_builder.of_body_part(body_part, state) {
                            if let Some(prev_state) = self.body_part_buffer.get_mut(&body_part) {
                                prev_state.transform =
                                    new_transform.to_matrix() * prev_state.transform;
                            } else {
                                self.body_part_buffer.insert(
                                    body_part,
                                    BodyPartInfo {
                                        transform: new_transform.to_matrix(),
                                        parent: body_part_builder.parent(),
                                    },
                                );
                            }
                        }
                    }
                }
            } else {
                animation_state_ac.remove(actor, walking_animation);
            }

            for (_, body_part, _) in
                body_part_amc.get_actor_model(crate::entity::actor_model::ActorModel(0))
            {
                let &BodyPartInfo {
                    mut transform,
                    mut parent,
                } = self
                    .body_part_buffer
                    .get(&body_part)
                    .unwrap_or(&BodyPartInfo {
                        transform: Mat4F32::IDENTITY,
                        parent: BASE_BODY_PART,
                    });

                while let Some(parent_info) = self.body_part_buffer.get(&parent) {
                    transform = parent_info.transform * transform;
                    parent = parent_info.parent;
                }
                transform = base_transform * transform;

                body_part_amc.get(actor_model, body_part).unwrap().build(
                    &position,
                    &transform,
                    &mut self.vertices,
                    &mut self.indices,
                );
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
