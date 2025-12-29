use crate::{
    assets::ACTOR_BLOCK_SHADER_PATH,
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
        actor_model::builder::BuilderActorModelComponent,
    },
    entity::actor_model::ActorBone,
    resource::{
        player_actor::PlayerActor,
        render_pool::{
            gpu_vec::GpuVec,
            new_quad_index_buffer,
            primitives::Vertex,
            IndexType,
            RenderParameters,
            Renderer,
            INDEX_FORMAT,
            INDEX_FORMAT_BYTE_SIZE,
            INITIAL_INDEX_BUFFER_LENGTH,
        },
    },
    window::Window,
};
use nohash_hasher::IntMap;
use std::time::Instant;
use voxbrix_common::{
    component::block::sky_light::{
        SkyLight,
        SkyLightBlockComponent,
    },
    entity::block::Block,
    math::{
        Directions,
        Mat4F32,
        QuatF32,
        Round,
        Vec3F32,
    },
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ActorRenderSystemDescriptor<'a> {
    pub render_parameters: RenderParameters<'a>,
    pub actor_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub actor_texture_bind_group: wgpu::BindGroup,
}

impl<'a> ActorRenderSystemDescriptor<'a> {
    pub async fn build(self, window: &Window) -> ActorRenderSystem {
        let Self {
            render_parameters:
                RenderParameters {
                    camera_bind_group_layout,
                },
            actor_texture_bind_group_layout,
            actor_texture_bind_group,
        } = self;

        let shader = voxbrix_common::read_file_async(ACTOR_BLOCK_SHADER_PATH)
            .await
            .expect("unable to read shader file");

        let shader =
            std::str::from_utf8(&shader).expect("unable to convert binary file to UTF-8 string");

        let shader = window
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("actor_shader"),
                source: wgpu::ShaderSource::Wgsl(shader.into()),
            });

        let render_pipeline_layout =
            window
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("actor_render_pipeline_layout"),
                    bind_group_layouts: &[
                        &camera_bind_group_layout,
                        &actor_texture_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        let render_pipeline =
            window
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("actor_render_pipeline"),
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
                            format: window.surface_view_format(),
                            blend: Some(wgpu::BlendState::REPLACE),
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

        let vertex_buffer = GpuVec::new(window.device(), wgpu::BufferUsages::VERTEX);
        let index_buffer =
            new_quad_index_buffer(window.device(), window.queue(), INITIAL_INDEX_BUFFER_LENGTH);

        ActorRenderSystem {
            render_pipeline,
            actor_texture_bind_group,
            bone_transformations: IntMap::default(),
            vertices: Vec::new(),
            vertex_buffer,
            index_buffer,
        }
    }
}

pub struct ActorRenderSystem {
    render_pipeline: wgpu::RenderPipeline,
    actor_texture_bind_group: wgpu::BindGroup,
    bone_transformations: IntMap<ActorBone, Mat4F32>,
    vertices: Vec<Vertex>,
    vertex_buffer: GpuVec,
    index_buffer: wgpu::Buffer,
}

impl System for ActorRenderSystem {
    type Data<'a> = ActorRenderSystemData<'a>;
}

#[derive(SystemData)]
pub struct ActorRenderSystemData<'a> {
    system: &'a mut ActorRenderSystem,
    player_actor: &'a PlayerActor,
    class_ac: &'a ClassActorComponent,
    position_ac: &'a PositionActorComponent,
    velocity_ac: &'a VelocityActorComponent,
    orientation_ac: &'a OrientationActorComponent,
    model_acc: &'a ModelActorClassComponent,
    builder_amc: &'a BuilderActorModelComponent,
    sky_light_bc: &'a SkyLightBlockComponent,
    animation_state_ac: &'a mut AnimationStateActorComponent,
}

impl ActorRenderSystemData<'_> {
    pub fn run(&mut self, renderer: Renderer) {
        // TODO can be different from player actor:
        let camera_orientation = self
            .orientation_ac
            .get(&self.player_actor.0)
            .expect("player actor orientation is undefined");

        self.system.vertices.clear();

        for (actor, position, model) in self
            .position_ac
            .iter()
            .filter(|(actor, _)| *actor != self.player_actor.0)
            .filter_map(|(actor, position)| {
                let class = self.class_ac.get(&actor)?;
                let model = self.model_acc.get(class, &actor)?;
                Some((actor, position, model))
            })
        {
            self.system.bone_transformations.clear();

            let model_builder = match self.builder_amc.get(model) {
                Some(s) => s,
                None => continue,
            };

            // Orientation
            // TODO swimming / wallclimbing / etc.
            let base_transform = if let Some(body_orient) =
                self.orientation_ac.get(&actor).and_then(|ori| {
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
            if self
                .velocity_ac
                .get(&actor)
                .filter(|vel| vel.vector.length() > f32::EPSILON)
                .is_some()
            {
                if model_builder.has_animation(&walking_animation) {
                    let state = match self.animation_state_ac.get(&actor, &walking_animation) {
                        Some(s) => s,
                        None => {
                            // TODO have common Instant::now()
                            self.animation_state_ac.insert(
                                actor,
                                walking_animation,
                                AnimationState {
                                    start: Instant::now(),
                                },
                            );
                            self.animation_state_ac
                                .get(&actor, &walking_animation)
                                .unwrap()
                        },
                    };

                    let state = (state.start.elapsed().as_millis() % walking_animation_duration_ms)
                        as f32
                        / walking_animation_duration_ms as f32;

                    for (bone, new_transform) in model_builder.list_bones().filter_map(|bp| {
                        let transform =
                            model_builder.animate_bone(bp, &walking_animation, state)?;

                        Some((bp, transform))
                    }) {
                        if let Some(prev_state) = self.system.bone_transformations.get_mut(&bone) {
                            *prev_state = new_transform.to_matrix() * *prev_state;
                        } else {
                            self.system
                                .bone_transformations
                                .insert(*bone, new_transform.to_matrix());
                        }
                    }
                }
            } else {
                self.animation_state_ac.remove(&actor, &walking_animation);
            }

            let sky_light = Block::from_chunk_offset(
                position.chunk,
                position.offset.to_array().map(|f| f.round_down()).into(),
            )
            .and_then(|(chunk, block)| self.sky_light_bc.get_chunk(&chunk).map(|c| *c.get(block)))
            .unwrap_or(SkyLight::MAX);

            let vertices_start = self.system.vertices.len();

            for bone in model_builder.list_bones() {
                let mut transform = Mat4F32::IDENTITY;
                let mut curr_bone = *bone;

                // Walt up through all parents and apply their transformations
                while let Some(param) = model_builder.get_bone_parameters(&curr_bone) {
                    if let Some(animation_transform) =
                        self.system.bone_transformations.get(&curr_bone)
                    {
                        transform = *animation_transform * transform;
                    }

                    // Go to next parent
                    transform = param.transformation * transform;
                    curr_bone = param.parent;
                }

                transform = base_transform * transform;

                model_builder.build_bone(
                    bone,
                    &position,
                    &transform,
                    &camera_orientation,
                    &mut self.system.vertices,
                );
            }

            self.system.vertices[vertices_start ..]
                .iter_mut()
                .for_each(|vertex| {
                    vertex.light_parameters =
                        (vertex.light_parameters & !0xFF) | (sky_light.value() as u32);
                });
        }

        // Actual rendering starts here
        let vertices_len: IndexType = self
            .system
            .vertices
            .len()
            .try_into()
            .expect("too many vertices");

        if vertices_len == 0 {
            return;
        }

        let vertex_buffer_byte_size = vertices_len as u64 * Vertex::size();

        let mut writer = self.system.vertex_buffer.get_writer(
            renderer.device,
            renderer.queue,
            vertex_buffer_byte_size,
        );

        writer
            .as_mut()
            .copy_from_slice(bytemuck::cast_slice(self.system.vertices.as_slice()));

        drop(writer);

        let quad_num = vertices_len / 4;

        let required_index_len = const { INDEX_FORMAT_BYTE_SIZE as u64 * 6 } * quad_num as u64;

        if self.system.index_buffer.size() < required_index_len {
            let size = required_index_len.max(self.system.index_buffer.size() * 2);

            self.system.index_buffer = new_quad_index_buffer(renderer.device, renderer.queue, size);
        }

        let mut render_pass = renderer.with_pipeline(&self.system.render_pipeline);

        render_pass.set_bind_group(1, &self.system.actor_texture_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.system.vertex_buffer.get_slice());
        let num_indices = quad_num * 6;
        render_pass.set_index_buffer(
            self.system
                .index_buffer
                .slice(.. num_indices as u64 * const { INDEX_FORMAT_BYTE_SIZE as u64 }),
            INDEX_FORMAT,
        );
        render_pass.draw_indexed(0 .. num_indices, 0, 0 .. 1);
    }
}
