use crate::{
    assets::SHADERS_PATH,
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

        let shaders = voxbrix_common::read_file_async(SHADERS_PATH)
            .await
            .expect("unable to read shaders file");

        let shaders =
            std::str::from_utf8(&shaders).expect("unable to convert binary file to UTF-8 string");

        let shaders = output_thread
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Actor Shaders"),
                source: wgpu::ShaderSource::Wgsl(shaders.into()),
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
                        entry_point: Some("vs_main"),
                        buffers: &[VertexDescription::desc(), Polygon::desc()],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shaders,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: texture_format,
                            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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
            bone_transformations: IntMap::default(),
            polygons: Vec::new(),
            vertex_buffer,
            polygon_buffer,
        }
    }
}

pub struct ActorRenderSystem {
    render_pipeline: wgpu::RenderPipeline,
    actor_texture_bind_group: wgpu::BindGroup,
    bone_transformations: IntMap<ActorBone, Mat4F32>,
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
            self.bone_transformations.clear();

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

                    for (bone, new_transform) in model_builder.list_bones().filter_map(|bp| {
                        let transform =
                            model_builder.animate_bone(bp, &walking_animation, state)?;

                        Some((bp, transform))
                    }) {
                        if let Some(prev_state) = self.bone_transformations.get_mut(&bone) {
                            *prev_state = new_transform.to_matrix() * *prev_state;
                        } else {
                            self.bone_transformations
                                .insert(*bone, new_transform.to_matrix());
                        }
                    }
                }
            } else {
                animation_state_ac.remove(&actor, &walking_animation);
            }

            for bone in model_builder.list_bones() {
                let mut transform = Mat4F32::IDENTITY;
                let mut curr_bone = *bone;

                // Walt up through all parents and apply their transformations
                while let Some(param) = model_builder.get_bone_parameters(&curr_bone) {
                    if let Some(animation_transform) = self.bone_transformations.get(&curr_bone) {
                        transform = *animation_transform * transform;
                    }

                    // Go to next parent
                    transform = param.transformation * transform;
                    curr_bone = param.parent;
                }

                transform = base_transform * transform;

                model_builder.build_bone(bone, &position, &transform, &mut self.polygons);
            }
        }
    }

    pub fn render(&mut self, renderer: Renderer) {
        let polygons_len = self.polygons.len();

        if polygons_len == 0 {
            return;
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
    }
}
