use crate::resource::render_pool::{
    gpu_vec::GpuVec,
    primitives::Vertex,
    IndexType,
    Renderer,
};
use ahash::{
    AHashMap,
    AHashSet,
};
use rayon::prelude::*;
use std::{
    collections::VecDeque,
    iter,
};
use voxbrix_common::entity::{
    block::{
        Block,
        Neighbor,
        BLOCKS_IN_CHUNK_EDGE_F32,
    },
    chunk::{
        Chunk,
        Dimension,
    },
};

pub struct VertexBuffer {
    num_vertices: IndexType,
    buffer: GpuVec,
}

impl VertexBuffer {
    pub fn get_slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.get_slice()
    }

    pub fn num_vertices(&self) -> IndexType {
        self.num_vertices
    }
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

pub struct RenderDataChunkComponent {
    block_change_queue: VecDeque<Chunk>,
    block_change_neighbors: AHashMap<Chunk, [bool; 6]>,
    chunk_queue: VecDeque<Chunk>,
    enqueued_chunks: AHashSet<Chunk>,
    chunk_buffer_shards: AHashMap<Chunk, Vec<Vertex>>,
    free_shards: Vec<Vec<Vertex>>,
    superchunk_side_size: i32,
    prepared_vertex_buffers: AHashMap<SuperChunk, VertexBuffer>,
    updated_vertex_buffers: AHashSet<SuperChunk>,
    free_vertex_buffers: Vec<VertexBuffer>,
}

impl RenderDataChunkComponent {
    pub fn new() -> Self {
        Self {
            block_change_queue: Default::default(),
            block_change_neighbors: Default::default(),
            chunk_queue: Default::default(),
            enqueued_chunks: Default::default(),
            chunk_buffer_shards: Default::default(),
            free_shards: Default::default(),
            superchunk_side_size: 4,
            prepared_vertex_buffers: Default::default(),
            updated_vertex_buffers: Default::default(),
            free_vertex_buffers: Default::default(),
        }
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

    pub fn is_queue_empty(&self) -> bool {
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

    pub fn select_chunks(
        &mut self,
        chunk_exists: impl Fn(&Chunk) -> bool,
    ) -> Vec<(Chunk, Vec<Vertex>)> {
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
            .filter(&chunk_exists)
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
                .filter(&chunk_exists)
                .map(get_shard)
                .take(to_add),
        );

        for (chunk, _) in selected_chunks.iter() {
            let superchunk = SuperChunk::of_chunk(self.superchunk_side_size, chunk);
            self.updated_vertex_buffers.insert(superchunk);
        }

        selected_chunks
    }

    pub fn submit_vertices(
        &mut self,
        par_iter: impl ParallelIterator<Item = (Chunk, Vec<Vertex>)>,
    ) {
        self.chunk_buffer_shards.par_extend(par_iter);
    }

    /// Returns maximum vertices for a single mesh among updated ones.
    pub fn prepare_render(&mut self, renderer: &Renderer) -> u32 {
        let mut max_vertices = 0;

        // Rendering:
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

                max_vertices = max_vertices.max(vertices_len);

                vertex_buffer.num_vertices = vertices_len;
            }
        }

        max_vertices
    }

    /// For the passed [`is_visible`] function:
    /// first argument is chunk position of the object,
    /// second - chunk offset of the object,
    /// third - radius of the object.
    pub fn get_visible_buffers<'a>(
        &'a self,
        is_visible: impl Fn([i32; 3], [f32; 3], f32) -> bool + 'a,
    ) -> impl Iterator<Item = &'a VertexBuffer> + 'a {
        self.prepared_vertex_buffers
            .iter()
            .filter(move |(superchunk, _)| {
                let (chunk, offset) = superchunk.center(self.superchunk_side_size);

                let radius = SuperChunk::radius(self.superchunk_side_size);

                is_visible(chunk.position, offset, radius)
            })
            .map(|(_, qb)| qb)
    }
}
