use crate::{
    assets::SHADERS_PATH,
    component::{
        block::class::ClassBlockComponent,
        block_class::model::ModelBlockClassComponent,
        block_model::{
            builder::{
                self,
                BuilderBlockModelComponent,
                CullFlags,
            },
            culling::{
                Culling,
                CullingBlockModelComponent,
            },
        },
    },
    system::{
        chunk_render_pipeline::ComputeContext,
        render::{
            gpu_vec::GpuVec,
            output_thread::OutputThread,
            primitives::{
                Polygon,
                VertexDescription,
            },
            RenderParameters,
            Renderer,
        },
    },
};
use ahash::AHashMap;
use arrayvec::ArrayVec;
use rayon::prelude::*;
use std::mem;
use voxbrix_common::{
    component::block::{
        sky_light::{
            SkyLight,
            SkyLightBlockComponent,
        },
        BlocksVec,
    },
    entity::{
        block::{
            Block,
            Neighbor,
            BLOCKS_IN_CHUNK,
        },
        block_class::BlockClass,
        chunk::Chunk,
    },
};
use wgpu::util::DeviceExt;

const POLYGON_SIZE: usize = Polygon::size() as usize;
const POLYGON_BUFFER_CAPACITY: usize = BLOCKS_IN_CHUNK * 6 /*sides*/;

fn neighbors_to_cull_flags(
    neighbors: &[Neighbor; 6],
    this_chunk: &BlocksVec<BlockClass>,
    neighbor_chunks: &[Option<&BlocksVec<BlockClass>>; 6],
    model_bcc: &ModelBlockClassComponent,
    culling_bmc: &CullingBlockModelComponent,
) -> CullFlags {
    let mut cull_flags = CullFlags::all();
    for (i, (neighbor, neighbor_chunk)) in neighbors.iter().zip(neighbor_chunks.iter()).enumerate()
    {
        let side = CullFlags::from_index(i);

        match neighbor {
            Neighbor::ThisChunk(n) => {
                let class = this_chunk.get(*n);
                let culling = model_bcc
                    .get(class)
                    .and_then(|model| culling_bmc.get(model));
                match culling {
                    Some(Culling::Full) => {
                        cull_flags.remove(side);
                    },
                    None => {},
                }
            },
            Neighbor::OtherChunk(n) => {
                if let Some(chunk) = neighbor_chunk {
                    let class = chunk.get(*n);
                    let culling = model_bcc
                        .get(class)
                        .and_then(|model| culling_bmc.get(model));
                    match culling {
                        Some(Culling::Full) => {
                            cull_flags.remove(side);
                        },
                        None => {},
                    }
                } else {
                    cull_flags.remove(side);
                }
            },
        }
    }

    cull_flags
}

struct ChunkInfo<'a> {
    chunk_shard: &'a Vec<Polygon>,
    polygon_length: usize,
    polygon_buffer: Option<&'a mut [u8]>,
}

fn slice_buffers<'a>(chunk_info: &mut [ChunkInfo<'a>], mut polygon_buffer: &'a mut [u8]) {
    for chunk in chunk_info.iter_mut() {
        let (polygon_buffer_shard, residue) =
            polygon_buffer.split_at_mut(chunk.polygon_length * POLYGON_SIZE);
        polygon_buffer = residue;
        chunk.polygon_buffer = Some(polygon_buffer_shard);
    }
}

pub struct BlockRenderSystemDescriptor<'a> {
    pub render_parameters: RenderParameters<'a>,
    pub block_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub block_texture_bind_group: wgpu::BindGroup,
}

impl<'a> BlockRenderSystemDescriptor<'a> {
    pub async fn build(self, output_thread: &OutputThread) -> BlockRenderSystem {
        let Self {
            render_parameters:
                RenderParameters {
                    camera_bind_group_layout,
                    texture_format,
                },
            block_texture_bind_group_layout,
            block_texture_bind_group,
        } = self;

        let shaders = voxbrix_common::read_file_async(SHADERS_PATH)
            .await
            .expect("unable to read shaders file");

        let shaders =
            std::str::from_utf8(&shaders).expect("unable to convert binary file to UTF-8 string");

        let shaders = output_thread
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Block Shaders"),
                source: wgpu::ShaderSource::Wgsl(shaders.into()),
            });

        let render_pipeline_layout =
            output_thread
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

        // Target block hightlighting
        let target_highlight_polygon_buffer =
            output_thread
                .device()
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Highlight Vertex Buffer"),
                    size: Polygon::size(),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });

        BlockRenderSystem {
            render_pipeline,
            chunk_buffer_shards: AHashMap::new(),
            update_chunk_buffer: false,
            prepared_vertex_buffer: vertex_buffer,
            prepared_polygon_buffer: polygon_buffer,
            num_polygons: 0,
            block_texture_bind_group,
            target_highlighting: TargetHighlighting::None,
            target_highlight_polygon_buffer,
        }
    }
}

enum TargetHighlighting {
    None,
    Previous,
    New(Polygon),
}

pub struct BlockRenderSystem {
    render_pipeline: wgpu::RenderPipeline,
    chunk_buffer_shards: AHashMap<Chunk, Vec<Polygon>>,
    update_chunk_buffer: bool,
    prepared_vertex_buffer: wgpu::Buffer,
    prepared_polygon_buffer: GpuVec,
    num_polygons: u32,
    block_texture_bind_group: wgpu::BindGroup,
    target_highlighting: TargetHighlighting,
    target_highlight_polygon_buffer: wgpu::Buffer,
}

impl BlockRenderSystem {
    fn build_chunk_buffer_shard(
        chunk: &Chunk,
        class_bc: &ClassBlockComponent,
        model_bcc: &ModelBlockClassComponent,
        builder_bmc: &BuilderBlockModelComponent,
        culling_bmc: &CullingBlockModelComponent,
        sky_light_bc: &SkyLightBlockComponent,
    ) -> Vec<Polygon> {
        let mut polygon_buffer = Vec::with_capacity(POLYGON_BUFFER_CAPACITY);

        let neighbor_chunk_ids = [
            [-1, 0, 0],
            [1, 0, 0],
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
        ]
        .map(|offset| chunk.checked_add(offset));

        let this_chunk_class = class_bc.get_chunk(chunk).unwrap();
        let this_chunk_light = sky_light_bc.get_chunk(chunk).unwrap();

        let neighbor_chunk_class = neighbor_chunk_ids.map(|chunk| {
            let block_classes = class_bc.get_chunk(&chunk?)?;

            Some(block_classes)
        });

        let neighbor_chunk_light = neighbor_chunk_ids.map(|chunk| {
            let block_light = sky_light_bc.get_chunk(&chunk?)?;

            Some(block_light)
        });

        for (block, block_class) in this_chunk_class.iter() {
            if let Some(model_builder) = model_bcc.get(block_class).and_then(|m| builder_bmc.get(m))
            {
                let neighbors = block.neighbors();

                let cull_flags = neighbors_to_cull_flags(
                    &neighbors,
                    this_chunk_class,
                    &neighbor_chunk_class,
                    model_bcc,
                    culling_bmc,
                );

                let sky_light_levels = neighbors
                    .iter()
                    .zip(neighbor_chunk_light)
                    .map(|(neighbor, neighbor_chunk_light)| {
                        Some(match neighbor {
                            Neighbor::ThisChunk(block) => *this_chunk_light.get(*block),
                            Neighbor::OtherChunk(block) => *neighbor_chunk_light?.get(*block),
                        })
                    })
                    .map(|light| light.unwrap_or(SkyLight::MIN).value())
                    .collect::<ArrayVec<_, 6>>()
                    .into_inner()
                    .unwrap_or_else(|_| unreachable!());

                model_builder.build(
                    &mut polygon_buffer,
                    chunk,
                    block,
                    cull_flags,
                    sky_light_levels,
                );
            }
        }

        polygon_buffer
    }

    // TODO move into component
    pub fn remove_chunk(&mut self, chunk: &Chunk) {
        self.chunk_buffer_shards.remove(chunk);
    }

    pub fn compute_chunks(
        &mut self,
        compute_context: ComputeContext<'_>,
        class_bc: &ClassBlockComponent,
        model_bcc: &ModelBlockClassComponent,
        builder_bmc: &BuilderBlockModelComponent,
        culling_bmc: &CullingBlockModelComponent,
        sky_light_bc: &SkyLightBlockComponent,
    ) {
        let chunk = compute_context.queue.next().unwrap();

        let par_iter = [
            [0, 0, 0],
            [-1, 0, 0],
            [1, 0, 0],
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
        ]
        .into_iter()
        .filter_map(|offset| {
            let chunk = chunk.checked_add(offset)?;
            class_bc.get_chunk(&chunk)?;
            sky_light_bc.get_chunk(&chunk)?;
            Some(chunk)
        })
        .par_bridge()
        .map(|chunk| {
            let shard = Self::build_chunk_buffer_shard(
                &chunk,
                class_bc,
                model_bcc,
                builder_bmc,
                culling_bmc,
                sky_light_bc,
            );

            (chunk, shard)
        });

        self.chunk_buffer_shards.par_extend(par_iter);

        self.update_chunk_buffer = true;
    }

    pub fn build_target_highlight(&mut self, target: Option<(Chunk, Block, usize)>) {
        if let Some((chunk, block, side)) = target {
            self.target_highlighting =
                TargetHighlighting::New(builder::side_highlighting(chunk.position, block, side));
        } else {
            self.target_highlighting = TargetHighlighting::None;
        }
    }

    pub fn render(&mut self, renderer: Renderer) {
        if self.update_chunk_buffer {
            let mut polygons_len = 0;

            let mut chunk_info = self
                .chunk_buffer_shards
                .values()
                .map(|polygons| {
                    polygons_len += polygons.len();

                    ChunkInfo {
                        chunk_shard: polygons,
                        polygon_length: polygons.len(),
                        polygon_buffer: None,
                    }
                })
                .collect::<Vec<_>>();

            let polygon_buffer_byte_size = (polygons_len * POLYGON_SIZE) as u64;

            if polygons_len != 0 {
                let mut writer = self.prepared_polygon_buffer.get_writer(
                    renderer.device,
                    renderer.queue,
                    polygon_buffer_byte_size,
                );

                slice_buffers(&mut chunk_info, writer.as_mut());

                chunk_info.par_iter_mut().for_each(|chunk| {
                    chunk
                        .polygon_buffer
                        .as_mut()
                        .unwrap()
                        .copy_from_slice(bytemuck::cast_slice(chunk.chunk_shard));
                });
            }

            self.prepared_polygon_buffer.finish();
            self.num_polygons = polygons_len as u32;
            self.update_chunk_buffer = false;
        }

        let queue = renderer.queue;

        let mut render_pass = renderer.with_pipeline(&mut self.render_pipeline);

        render_pass.set_bind_group(1, &self.block_texture_bind_group, &[]);

        if !self.prepared_polygon_buffer.is_empty() {
            render_pass.set_vertex_buffer(0, self.prepared_vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.prepared_polygon_buffer.get_slice());
            render_pass.draw(0 .. 6, 0 .. self.num_polygons);
        }

        let target_highlighting =
            mem::replace(&mut self.target_highlighting, TargetHighlighting::Previous);

        if !matches!(target_highlighting, TargetHighlighting::None) {
            if let TargetHighlighting::New(polygon) = target_highlighting {
                queue.write_buffer(
                    &self.target_highlight_polygon_buffer,
                    0,
                    bytemuck::cast_slice(&[polygon]),
                );
            }

            render_pass.set_vertex_buffer(0, self.prepared_vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.target_highlight_polygon_buffer.slice(..));
            render_pass.draw(0 .. 6, 0 .. 1);
        }
    }
}
