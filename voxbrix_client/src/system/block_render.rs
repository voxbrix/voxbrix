use crate::{
    assets::SHADERS_PATH,
    component::{
        block::class::ClassBlockComponent,
        block_class::model::ModelBlockClassComponent,
        block_model::{
            builder::{
                BuilderBlockModelComponent,
                CullFlags,
            },
            culling::{
                Culling,
                CullingBlockModelComponent,
            },
        },
        texture::location::LocationTextureComponent,
    },
    entity::texture::Texture,
    system::render::{
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
    window::Window,
};
use ahash::{
    AHashMap,
    AHashSet,
};
use arrayvec::ArrayVec;
use rayon::prelude::*;
use std::{
    collections::VecDeque,
    iter,
    mem,
};
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
            BLOCKS_IN_CHUNK_EDGE_F32,
        },
        block_class::BlockClass,
        chunk::{
            Chunk,
            Dimension,
        },
    },
    LabelMap,
};

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
    chunk_shard: &'a Vec<Vertex>,
    vertex_length: usize,
    vertex_buffer: Option<&'a mut [u8]>,
}

fn slice_buffers<'a>(chunk_info: &mut [ChunkInfo<'a>], mut vertex_buffer: &'a mut [u8]) {
    for chunk in chunk_info.iter_mut() {
        let (vertex_buffer_shard, residue) =
            vertex_buffer.split_at_mut(chunk.vertex_length * const { Vertex::size() as usize });
        vertex_buffer = residue;
        chunk.vertex_buffer = Some(vertex_buffer_shard);
    }
}

pub struct BlockRenderSystemDescriptor<'a> {
    pub render_parameters: RenderParameters<'a>,
    pub block_texture_bind_group_layout: wgpu::BindGroupLayout,
    pub block_texture_bind_group: wgpu::BindGroup,
    pub block_texture_label_map: LabelMap<Texture>,
    pub location_tc: &'a LocationTextureComponent,
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
            block_texture_label_map,
            location_tc,
        } = self;

        let shaders = voxbrix_common::read_file_async(SHADERS_PATH)
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
                    push_constant_ranges: &[],
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
        let highlight_texture_index = location_tc.get_index(highlight_texture);
        let highlight_texture_coords = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
            .map(|coords| location_tc.get_coords(highlight_texture, coords));

        let index_buffer =
            new_quad_index_buffer(window.device(), window.queue(), INITIAL_INDEX_BUFFER_LENGTH);

        BlockRenderSystem {
            block_change_queue: VecDeque::new(),
            block_change_neighbors: AHashMap::new(),
            chunk_queue: VecDeque::new(),
            enqueued_chunks: AHashSet::new(),
            render_pipeline,
            chunk_buffer_shards: AHashMap::new(),
            free_shards: Vec::new(),
            prepared_vertex_buffers: AHashMap::new(),
            updated_vertex_buffers: AHashSet::new(),
            index_buffer,
            superchunk_side_size: 4,
            free_vertex_buffers: Vec::new(),
            block_texture_bind_group,
            target_highlighting: TargetHighlighting::None,
            target_highlight_vertex_buffer,
            highlight_texture_index,
            highlight_texture_coords,
        }
    }
}

enum TargetHighlighting {
    None,
    Previous,
    New([Vertex; 4]),
}

struct VertexBuffer {
    num_vertices: IndexType,
    buffer: GpuVec,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct SuperChunk {
    position: [i32; 3],
    dimension: Dimension,
}

impl SuperChunk {
    fn chunks(&self, side: i32) -> impl Iterator<Item = Chunk> {
        let chunk_coord_base = self.position.map(|i| i * side);
        let dimension = self.dimension;

        (0 .. side)
            .flat_map(move |z| (0 .. side).flat_map(move |y| (0 .. side).map(move |x| [x, y, z])))
            .map(move |add| {
                let position = [0, 1, 2].map(|i| chunk_coord_base[i] + add[i]);

                Chunk {
                    position,
                    dimension,
                }
            })
    }

    fn center(&self, side: i32) -> (Chunk, [f32; 3]) {
        let center_chunk = self.position.map(|i| i * side + side / 2);

        let center_offset = [(side % 2) as f32 * BLOCKS_IN_CHUNK_EDGE_F32; 3];

        (
            Chunk {
                position: center_chunk,
                dimension: self.dimension,
            },
            center_offset,
        )
    }

    /// Max (diagonal) radius, in blocks
    fn radius(side: i32) -> f32 {
        // Cube diagonal is side * 3.0.sqrt()
        const COEF: f32 = 1.73205077648162841796875f32 * BLOCKS_IN_CHUNK_EDGE_F32;
        side as f32 * COEF
    }

    fn of_chunk(side: i32, chunk: &Chunk) -> Self {
        let position = chunk.position.map(|i| {
            let mut quot = i / side;
            let rem = i % side;

            if i < 0 && rem != 0 {
                quot -= 1;
            }

            quot
        });

        Self {
            position,
            dimension: chunk.dimension,
        }
    }
}

pub struct BlockRenderSystem {
    block_change_queue: VecDeque<Chunk>,
    block_change_neighbors: AHashMap<Chunk, [bool; 6]>,
    chunk_queue: VecDeque<Chunk>,
    enqueued_chunks: AHashSet<Chunk>,
    render_pipeline: wgpu::RenderPipeline,
    chunk_buffer_shards: AHashMap<Chunk, Vec<Vertex>>,
    free_shards: Vec<Vec<Vertex>>,
    superchunk_side_size: i32,
    prepared_vertex_buffers: AHashMap<SuperChunk, VertexBuffer>,
    updated_vertex_buffers: AHashSet<SuperChunk>,
    index_buffer: wgpu::Buffer,
    free_vertex_buffers: Vec<VertexBuffer>,
    block_texture_bind_group: wgpu::BindGroup,
    target_highlighting: TargetHighlighting,
    target_highlight_vertex_buffer: wgpu::Buffer,
    highlight_texture_index: u32,
    highlight_texture_coords: [[f32; 2]; 4],
}

impl BlockRenderSystem {
    fn build_chunk_buffer_shard<'a>(
        chunk: &'a Chunk,
        class_bc: &'a ClassBlockComponent,
        model_bcc: &'a ModelBlockClassComponent,
        builder_bmc: &'a BuilderBlockModelComponent,
        culling_bmc: &'a CullingBlockModelComponent,
        sky_light_bc: &'a SkyLightBlockComponent,
    ) -> impl ParallelIterator<Item = Vertex> + 'a {
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

        this_chunk_class
            .par_iter()
            .flat_map_iter(move |(block, block_class)| {
                model_bcc
                    .get(block_class)
                    .and_then(|m| builder_bmc.get(m))
                    .into_iter()
                    .flat_map(move |model_builder| {
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
                                    Neighbor::OtherChunk(block) => {
                                        *neighbor_chunk_light?.get(*block)
                                    },
                                })
                            })
                            .map(|light| light.unwrap_or(SkyLight::MIN))
                            .collect::<ArrayVec<_, 6>>()
                            .into_inner()
                            .unwrap_or_else(|_| unreachable!());

                        model_builder.build(chunk, block, cull_flags, sky_light_levels)
                    })
            })
    }

    /// Add Chunk into the low-priority queue, without taking care of neighbors
    /// should be used for adding new chunks after they are processed through other systems.
    /// The previous steps should take care and manually add neighbors if necessary.
    pub fn enqueue_chunk(&mut self, chunk: Chunk) {
        // Ignore if already exists in either queue.
        if self.block_change_neighbors.contains_key(&chunk) || !self.enqueued_chunks.insert(chunk) {
            return;
        }

        self.chunk_queue.push_back(chunk);
    }

    /// Block changes enqueued into high-priority queue.
    /// Also re-renders neighbor chunks if necessary.
    pub fn block_change(&mut self, chunk: &Chunk, block: Block) {
        let mut neighbor_needs_render = [false; 6];

        let neighbor_chunks = [
            [-1, 0, 0],
            [1, 0, 0],
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
        ]
        .into_iter()
        .map(|offset| chunk.checked_add(offset))
        .enumerate()
        .filter_map(|(side, chunk)| Some((side, chunk?)));

        let neighbors = block.neighbors();

        for (side, _chunk) in neighbor_chunks {
            if let Neighbor::OtherChunk(_) = neighbors[side] {
                neighbor_needs_render[side] = true;
            }
        }

        if let Some(prev) = self.block_change_neighbors.get(chunk) {
            // Add to-be-rendered neighbors instead of replacing existing
            for i in 0 .. 6 {
                neighbor_needs_render[i] = neighbor_needs_render[i] || prev[i];
            }
        } else {
            self.block_change_queue.push_back(*chunk);
            self.block_change_neighbors
                .insert(*chunk, neighbor_needs_render);
            // This queue is high priority, remove from the other one
            self.enqueued_chunks.remove(chunk);
        }
    }

    pub fn is_queue_empty(&mut self) -> bool {
        self.enqueued_chunks.is_empty() && self.block_change_neighbors.is_empty()
    }

    pub fn remove_chunk(&mut self, chunk: &Chunk) {
        self.enqueued_chunks.remove(chunk);
        self.block_change_neighbors.remove(chunk);
        if let Some(shard) = self.chunk_buffer_shards.remove(chunk) {
            self.free_shards.push(shard);
            let superchunk = SuperChunk::of_chunk(self.superchunk_side_size, chunk);
            if superchunk
                .chunks(self.superchunk_side_size)
                .find(|chunk| self.chunk_buffer_shards.contains_key(chunk))
                .is_none()
            {
                if let Some(vertex_buffer) = self.prepared_vertex_buffers.remove(&superchunk) {
                    self.free_vertex_buffers.push(vertex_buffer);
                }
                self.updated_vertex_buffers.remove(&superchunk);
            }
        }
    }

    pub fn process(
        &mut self,
        class_bc: &ClassBlockComponent,
        model_bcc: &ModelBlockClassComponent,
        builder_bmc: &BuilderBlockModelComponent,
        culling_bmc: &CullingBlockModelComponent,
        sky_light_bc: &SkyLightBlockComponent,
    ) {
        let chunk_exists = |chunk: &Chunk| -> bool {
            class_bc.get_chunk(chunk).is_some() && sky_light_bc.get_chunk(chunk).is_some()
        };

        let mut get_shard = |chunk: Chunk| -> (Chunk, Vec<Vertex>) {
            let mut shard = self
                .chunk_buffer_shards
                .remove(&chunk)
                .or_else(|| self.free_shards.pop())
                .unwrap_or_default();

            shard.clear();

            (chunk, shard)
        };

        let mut selected_chunks = iter::from_fn(|| self.block_change_queue.pop_front())
            .filter_map(|chunk| self.block_change_neighbors.get_key_value(&chunk))
            .flat_map(|(chunk, neighbors)| {
                let offsets = [
                    [-1, 0, 0],
                    [1, 0, 0],
                    [0, -1, 0],
                    [0, 1, 0],
                    [0, 0, -1],
                    [0, 0, 1],
                ];

                let neighbor_iter = neighbors.into_iter().zip(offsets.into_iter()).filter_map(
                    |(needs_render, offset)| {
                        if !needs_render {
                            return None;
                        }

                        chunk.checked_add(offset)
                    },
                );

                iter::once(*chunk).chain(neighbor_iter)
            })
            .filter(chunk_exists)
            .map(&mut get_shard)
            .collect::<Vec<_>>();

        for (chunk, _) in selected_chunks.iter() {
            self.block_change_neighbors.remove(chunk);
            self.enqueued_chunks.remove(chunk);
        }

        // Add some from non-priority queue
        let to_add = rayon::current_num_threads()
            .saturating_sub(2)
            .max(1)
            .saturating_sub(selected_chunks.len());

        selected_chunks.extend(
            iter::from_fn(|| self.chunk_queue.pop_front())
                .filter(|chunk| self.enqueued_chunks.remove(chunk))
                .filter(chunk_exists)
                .map(get_shard)
                .take(to_add),
        );

        for (chunk, _) in selected_chunks.iter() {
            let superchunk = SuperChunk::of_chunk(self.superchunk_side_size, chunk);
            self.updated_vertex_buffers.insert(superchunk);
        }

        let par_iter = selected_chunks.into_par_iter().map(|(chunk, mut shard)| {
            shard.par_extend(Self::build_chunk_buffer_shard(
                &chunk,
                class_bc,
                model_bcc,
                builder_bmc,
                culling_bmc,
                sky_light_bc,
            ));

            (chunk, shard)
        });

        self.chunk_buffer_shards.par_extend(par_iter);
    }

    pub fn build_target_highlight(&mut self, target: Option<(Chunk, Block, usize)>) {
        if let Some((chunk, block, side)) = target {
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
                    chunk: chunk.position,
                    texture_index: self.highlight_texture_index,
                    offset: positions[i],
                    texture_position: self.highlight_texture_coords[i],
                    light_parameters: 0,
                }
            });

            self.target_highlighting = TargetHighlighting::New(vertex);
        } else {
            self.target_highlighting = TargetHighlighting::None;
        }
    }

    pub fn render(&mut self, renderer: Renderer) {
        for superchunk in self.updated_vertex_buffers.drain() {
            let mut vertices_len = 0;

            let mut chunk_info = superchunk
                .chunks(self.superchunk_side_size)
                .filter_map(|chunk| self.chunk_buffer_shards.get(&chunk))
                .map(|vertices| {
                    vertices_len += vertices.len();

                    ChunkInfo {
                        chunk_shard: vertices,
                        vertex_length: vertices.len(),
                        vertex_buffer: None,
                    }
                })
                .collect::<Vec<_>>();

            let vertex_buffer_byte_size = vertices_len as u64 * Vertex::size();
            let vertices_len: IndexType = vertices_len.try_into().unwrap();

            if vertices_len == 0 {
                if let Some(vertex_buffer) = self.prepared_vertex_buffers.remove(&superchunk) {
                    self.free_vertex_buffers.push(vertex_buffer);
                }
            } else {
                let vertex_buffer = self
                    .prepared_vertex_buffers
                    .entry(superchunk)
                    .or_insert_with(|| {
                        self.free_vertex_buffers.pop().unwrap_or_else(|| {
                            VertexBuffer {
                                num_vertices: vertices_len,
                                buffer: GpuVec::new(renderer.device, wgpu::BufferUsages::VERTEX),
                            }
                        })
                    });

                let mut writer = vertex_buffer.buffer.get_writer(
                    renderer.device,
                    renderer.queue,
                    vertex_buffer_byte_size,
                );

                slice_buffers(&mut chunk_info, writer.as_mut());

                chunk_info.par_iter_mut().for_each(|chunk| {
                    chunk
                        .vertex_buffer
                        .as_mut()
                        .unwrap()
                        .copy_from_slice(bytemuck::cast_slice(chunk.chunk_shard));
                });

                let quad_num = vertices_len / 4;

                let required_index_len =
                    const { INDEX_FORMAT_BYTE_SIZE as u64 * 6 } * quad_num as u64;

                if self.index_buffer.size() < required_index_len {
                    let size = required_index_len.max(self.index_buffer.size() * 2);

                    self.index_buffer =
                        new_quad_index_buffer(renderer.device, renderer.queue, size);
                }

                vertex_buffer.num_vertices = vertices_len;
            }
        }

        let queue = renderer.queue;

        let buffers_to_render = self
            .prepared_vertex_buffers
            .iter()
            .filter(|(superchunk, _)| {
                let (chunk, offset) = superchunk.center(self.superchunk_side_size);

                let radius = SuperChunk::radius(self.superchunk_side_size);

                renderer
                    .camera
                    .is_object_visible(chunk.position, offset, radius)
            })
            .map(|(_, qb)| qb);

        let mut render_pass = renderer.with_pipeline(&mut self.render_pipeline);

        render_pass.set_bind_group(1, &self.block_texture_bind_group, &[]);

        for vertex_buffer in buffers_to_render {
            render_pass.set_vertex_buffer(0, vertex_buffer.buffer.get_slice());
            let num_indices = vertex_buffer.num_vertices / 4 * 6;
            render_pass.set_index_buffer(
                self.index_buffer
                    .slice(.. num_indices as u64 * const { INDEX_FORMAT_BYTE_SIZE as u64 }),
                INDEX_FORMAT,
            );
            render_pass.draw_indexed(0 .. num_indices, 0, 0 .. 1);
        }

        let target_highlighting =
            mem::replace(&mut self.target_highlighting, TargetHighlighting::Previous);

        if !matches!(target_highlighting, TargetHighlighting::None) {
            if let TargetHighlighting::New(vertex) = target_highlighting {
                queue.write_buffer(
                    &self.target_highlight_vertex_buffer,
                    0,
                    bytemuck::cast_slice(&vertex),
                );
            }

            render_pass.set_vertex_buffer(0, self.target_highlight_vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.index_buffer
                    .slice(.. 6 * const { INDEX_FORMAT_BYTE_SIZE as u64 }),
                INDEX_FORMAT,
            );
            render_pass.draw_indexed(0 .. 6, 0, 0 .. 1);
        }
    }
}
