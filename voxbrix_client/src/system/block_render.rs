use crate::{
    assets::ACTOR_BLOCK_SHADER_PATH,
    component::chunk::render_data::BlkRenderDataChunkComponent,
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

pub struct BlockRenderSystemDescriptor<'a> {
    pub render_parameters: RenderParameters<'a>,
    pub block_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub block_texture_bind_group: wgpu::BindGroup,
}

impl<'a> BlockRenderSystemDescriptor<'a> {
    pub async fn build(self, window: &Window) -> BlockRenderSystem {
        let Self {
            render_parameters:
                RenderParameters {
                    camera_bind_group_layout,
                    texture_format,
                },
            block_texture_bind_group_layout,
            block_texture_bind_group,
        } = self;

        let shader = voxbrix_common::read_file_async(ACTOR_BLOCK_SHADER_PATH)
            .await
            .expect("unable to read shader file");

        let shader =
            std::str::from_utf8(&shader).expect("unable to convert binary file to UTF-8 string");

        let shader = window
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Block Shaders"),
                source: wgpu::ShaderSource::Wgsl(shader.into()),
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
                    push_constant_ranges: &[],
                });

        let render_pipeline =
            window
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Render Pipeline"),
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
                            format: texture_format,
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

        let index_buffer =
            new_quad_index_buffer(window.device(), window.queue(), INITIAL_INDEX_BUFFER_LENGTH);

        BlockRenderSystem {
            render_pipeline,
            index_buffer,
            block_texture_bind_group,
        }
    }
}

pub struct BlockRenderSystem {
    render_pipeline: wgpu::RenderPipeline,
    index_buffer: wgpu::Buffer,
    block_texture_bind_group: wgpu::BindGroup,
}

impl System for BlockRenderSystem {
    type Data<'a> = BlockRenderSystemData<'a>;
}

#[derive(SystemData)]
pub struct BlockRenderSystemData<'a> {
    system: &'a mut BlockRenderSystem,
    render_data_cc: &'a mut BlkRenderDataChunkComponent,
}

impl BlockRenderSystemData<'_> {
    pub fn run(&mut self, renderer: Renderer) {
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

        let mut render_pass = renderer.with_pipeline(&mut self.system.render_pipeline);

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
    }
}
