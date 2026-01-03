use crate::{
    assets::BLOCK_SHADER_PATH,
    component::{
        actor::{
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
        },
        block::class::ClassBlockComponent,
    },
    entity::texture::Texture,
    resource::{
        player_actor::PlayerActor,
        render_pool::{
            new_quad_index_buffer,
            primitives::block::{
                Vertex,
                VertexConstants,
            },
            RenderParameters,
            Renderer,
            INDEX_FORMAT,
            INDEX_FORMAT_BYTE_SIZE,
            INITIAL_INDEX_BUFFER_LENGTH,
        },
    },
    window::Window,
};
use voxbrix_common::{
    component::block_class::collision::CollisionBlockClassComponent,
    system::position::get_target_block,
    LabelMap,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct TargetBlockHightlightSystemDescriptor<'a> {
    pub render_parameters: RenderParameters<'a>,
    pub block_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub block_texture_bind_group: wgpu::BindGroup,
    pub block_texture_label_map: LabelMap<Texture>,
}

impl<'a> TargetBlockHightlightSystemDescriptor<'a> {
    pub async fn build(self, window: &Window) -> TargetBlockHightlightSystem {
        let Self {
            render_parameters:
                RenderParameters {
                    camera_bind_group_layout,
                },
            block_texture_bind_group_layout,
            block_texture_bind_group,
            block_texture_label_map,
        } = self;

        let shaders = voxbrix_common::read_file_async(BLOCK_SHADER_PATH)
            .await
            .expect("unable to read shaders file");

        let shaders =
            std::str::from_utf8(&shaders).expect("unable to convert binary file to UTF-8 string");

        let shaders = window
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Block Shaders"),
                source: wgpu::ShaderSource::Wgsl(shaders.into()),
            });

        let render_pipeline_layout =
            window
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Render Pipeline Layout"),
                    bind_group_layouts: &[
                        &camera_bind_group_layout,
                        &block_texture_bind_group_layout,
                    ],
                    push_constant_ranges: &[wgpu::PushConstantRange {
                        range: 0 .. VertexConstants::size_bytes(),
                        stages: wgpu::ShaderStages::VERTEX,
                    }],
                });

        let render_pipeline =
            window
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
                    layout: Some(&render_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shaders,
                        entry_point: Some("vs_main"),
                        buffers: &[Vertex::desc()],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shaders,
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

        // Target block hightlighting
        let target_highlight_vertex_buffer =
            window.device().create_buffer(&wgpu::BufferDescriptor {
                label: Some("Highlight Vertex Buffer"),
                size: Vertex::size() * 4,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

        let highlight_texture = block_texture_label_map
            .get("highlight")
            .expect("highlight texture is missing");
        let highlight_texture_coords = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

        let index_buffer =
            new_quad_index_buffer(window.device(), window.queue(), INITIAL_INDEX_BUFFER_LENGTH);

        TargetBlockHightlightSystem {
            render_pipeline,
            index_buffer,
            block_texture_bind_group,
            target_highlight_vertex_buffer,
            highlight_texture_index: highlight_texture.as_u32(),
            highlight_texture_coords,
        }
    }
}

pub struct TargetBlockHightlightSystem {
    render_pipeline: wgpu::RenderPipeline,
    index_buffer: wgpu::Buffer,
    block_texture_bind_group: wgpu::BindGroup,
    target_highlight_vertex_buffer: wgpu::Buffer,
    highlight_texture_index: u32,
    highlight_texture_coords: [[f32; 2]; 4],
}

impl System for TargetBlockHightlightSystem {
    type Data<'a> = TargetBlockHightlightSystemData<'a>;
}

#[derive(SystemData)]
pub struct TargetBlockHightlightSystemData<'a> {
    system: &'a mut TargetBlockHightlightSystem,
    player_actor: &'a PlayerActor,
    position_ac: &'a PositionActorComponent,
    orientation_ac: &'a OrientationActorComponent,
    class_bc: &'a ClassBlockComponent,
    collision_bcc: &'a CollisionBlockClassComponent,
}

impl TargetBlockHightlightSystemData<'_> {
    pub fn run(&mut self, renderer: Renderer) {
        let Some(player_position) = self.position_ac.get(&self.player_actor.0) else {
            return;
        };

        let Some(player_orientation) = self.orientation_ac.get(&self.player_actor.0) else {
            return;
        };

        let Some((chunk, block, side)) = get_target_block(
            player_position,
            player_orientation.forward(),
            |chunk, block| {
                // TODO: better targeting collision?
                self.class_bc
                    .get_chunk(&chunk)
                    .map(|blocks| {
                        let class = blocks.get(block);
                        self.collision_bcc.get(class).is_some()
                    })
                    .unwrap_or(false)
            },
        ) else {
            return;
        };

        const ELEVATION: f32 = 0.01;

        let [x, y, z] = block.into_coords();

        let positions = match side {
            0 => [[x, y, z + 1], [x, y + 1, z + 1], [x, y + 1, z], [x, y, z]],
            1 => {
                [
                    [x + 1, y + 1, z + 1],
                    [x + 1, y, z + 1],
                    [x + 1, y, z],
                    [x + 1, y + 1, z],
                ]
            },
            2 => [[x + 1, y, z + 1], [x, y, z + 1], [x, y, z], [x + 1, y, z]],
            3 => {
                [
                    [x, y + 1, z + 1],
                    [x + 1, y + 1, z + 1],
                    [x + 1, y + 1, z],
                    [x, y + 1, z],
                ]
            },
            4 => [[x, y, z], [x, y + 1, z], [x + 1, y + 1, z], [x + 1, y, z]],
            5 => {
                [
                    [x + 1, y, z + 1],
                    [x + 1, y + 1, z + 1],
                    [x, y + 1, z + 1],
                    [x, y, z + 1],
                ]
            },
            _ => panic!("build_target_hightlight: incorrect side index"),
        };

        let (change_axis, change_amount) = match side {
            0 => (0, -ELEVATION),
            1 => (0, ELEVATION),
            2 => (1, -ELEVATION),
            3 => (1, ELEVATION),
            4 => (2, -ELEVATION),
            5 => (2, ELEVATION),
            _ => unreachable!(),
        };

        let positions = positions.map(|a| {
            let mut result = a.map(|i| i as f32);
            result[change_axis] += change_amount;
            result
        });

        let vertex = [0, 1, 2, 3].map(|i| {
            Vertex {
                offset: positions[i],
                texture_index: self.system.highlight_texture_index,
                texture_position: self.system.highlight_texture_coords[i],
                light_parameters: 0,
            }
        });

        let queue = renderer.queue;

        let mut render_pass = renderer.with_pipeline(&mut self.system.render_pipeline);

        render_pass.set_bind_group(1, &self.system.block_texture_bind_group, &[]);

        queue.write_buffer(
            &self.system.target_highlight_vertex_buffer,
            0,
            bytemuck::cast_slice(&vertex),
        );

        render_pass.set_push_constants(
            wgpu::ShaderStages::VERTEX,
            0,
            bytemuck::bytes_of(&VertexConstants {
                chunk: chunk.position.into(),
            }),
        );

        render_pass.set_vertex_buffer(0, self.system.target_highlight_vertex_buffer.slice(..));
        render_pass.set_index_buffer(
            self.system
                .index_buffer
                .slice(.. 6 * const { INDEX_FORMAT_BYTE_SIZE as u64 }),
            INDEX_FORMAT,
        );
        render_pass.draw_indexed(0 .. 6, 0, 0 .. 1);
    }
}
