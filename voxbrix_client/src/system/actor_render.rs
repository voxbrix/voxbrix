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
        actor_class::model::ModelActorClassComponent,
        actor_model::builder::{
            BuilderActorModelComponent,
            BASE_BODY_PART,
        },
    },
    entity::actor_model::ActorBodyPart,
    system::render::{
        gpu_vec::GpuVec,
        output_thread::OutputThread,
        primitives::{
            Polygon,
            VertexDescription,
        },
        RenderParameters,
        Renderer,
    },
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

const POLYGON_SIZE: usize = Polygon::size() as usize;

pub struct ActorRenderSystemDescriptor<'a> {
    pub render_parameters: RenderParameters<'a>,
    pub actor_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub actor_texture_bind_group: wgpu::BindGroup,
}

impl<'a> ActorRenderSystemDescriptor<'a> {
    pub async fn build(self, output_thread: &OutputThread) -> ActorRenderSystem {
        let Self {
            render_parameters:
                RenderParameters {
                    camera_bind_group_layout,
                    texture_format,
                },
            actor_texture_bind_group_layout,
            actor_texture_bind_group,
        } = self;

        let shaders = output_thread
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Shaders"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../shaders.wgsl").into()),
            });

        let render_pipeline_layout =
            output_thread
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[
                        &camera_bind_group_layout,
                        &actor_texture_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        let render_pipeline =
            output_thread
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(&render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shaders,
                        entry_point: "vs_main",
                        buffers: &[VertexDescription::desc(), Polygon::desc()],
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

        let vertex_buffer =
            output_thread
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Vertex Buffer"),
                    usage: wgpu::BufferUsages::VERTEX,
                    contents: bytemuck::cast_slice(&[
                        VertexDescription { index: 0 },
                        VertexDescription { index: 1 },
                        VertexDescription { index: 3 },
                        VertexDescription { index: 2 },
                        VertexDescription { index: 3 },
                        VertexDescription { index: 1 },
                    ]),
                });

        let polygon_buffer = GpuVec::new(output_thread.device(), wgpu::BufferUsages::VERTEX);

        ActorRenderSystem {
            render_pipeline,
            actor_texture_bind_group,
            body_part_buffer: IntMap::default(),
            polygons: Vec::new(),
            vertex_buffer,
            polygon_buffer,
        }
    }
}

struct BodyPartInfo {
    transform: Mat4F32,
    parent: ActorBodyPart,
}

pub struct ActorRenderSystem {
    render_pipeline: wgpu::RenderPipeline,
    actor_texture_bind_group: wgpu::BindGroup,
    body_part_buffer: IntMap<ActorBodyPart, BodyPartInfo>,
    polygons: Vec<Polygon>,
    vertex_buffer: wgpu::Buffer,
    polygon_buffer: GpuVec,
}

impl ActorRenderSystem {
    pub fn update(
        &mut self,
        player_actor: Actor,
        class_ac: &ClassActorComponent,
        position_ac: &PositionActorComponent,
        velocity_ac: &VelocityActorComponent,
        orientation_ac: &OrientationActorComponent,
        model_acc: &ModelActorClassComponent,
        builder_amc: &BuilderActorModelComponent,
        animation_state_ac: &mut AnimationStateActorComponent,
    ) {
        self.polygons.clear();

        for (actor, position, model) in position_ac
            .iter()
            .filter(|(actor, _)| *actor != player_actor)
            .filter_map(|(actor, position)| {
                let class = class_ac.get(&actor)?;
                let model = model_acc.get(&actor, class)?;
                Some((actor, position, model))
            })
        {
            self.body_part_buffer.clear();

            let model_builder = match builder_amc.get(model) {
                Some(s) => s,
                None => continue,
            };

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
                if model_builder.has_animation(&walking_animation) {
                    let state = match animation_state_ac.get(&actor, &walking_animation) {
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
                            animation_state_ac.get(&actor, &walking_animation).unwrap()
                        },
                    };

                    let state = (state.start.elapsed().as_millis() % walking_animation_duration_ms)
                        as f32
                        / walking_animation_duration_ms as f32;

                    for (body_part, new_transform) in
                        model_builder.list_body_parts().filter_map(|bp| {
                            let transform =
                                model_builder.animate_body_part(bp, &walking_animation, state)?;

                            Some((bp, transform))
                        })
                    {
                        if let Some(prev_state) = self.body_part_buffer.get_mut(&body_part) {
                            prev_state.transform = new_transform.to_matrix() * prev_state.transform;
                        } else {
                            self.body_part_buffer.insert(
                                *body_part,
                                BodyPartInfo {
                                    transform: new_transform.to_matrix(),
                                    parent: model_builder.get_body_part_parent(body_part).unwrap(),
                                },
                            );
                        }
                    }
                }
            } else {
                animation_state_ac.remove(&actor, &walking_animation);
            }

            for body_part in model_builder.list_body_parts() {
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

                model_builder.build_body_part(body_part, &position, &transform, &mut self.polygons);
            }
        }
    }

    pub fn render(&mut self, renderer: Renderer) -> Result<(), wgpu::SurfaceError> {
        let polygons_len = self.polygons.len();

        if polygons_len == 0 {
            return Ok(());
        }

        let polygon_buffer_byte_size = (polygons_len * POLYGON_SIZE) as u64;

        let mut writer = self.polygon_buffer.get_writer(
            renderer.device,
            renderer.queue,
            polygon_buffer_byte_size,
        );

        writer
            .as_mut()
            .copy_from_slice(bytemuck::cast_slice(self.polygons.as_slice()));

        drop(writer);

        self.polygon_buffer.finish();

        let mut render_pass = renderer.with_pipeline(&self.render_pipeline);

        render_pass.set_bind_group(1, &self.actor_texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.polygon_buffer.get_slice());
        render_pass.draw(0 .. 6, 0 .. polygons_len as u32);

        Ok(())
    }
}
