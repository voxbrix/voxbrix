use crate::resource::render_pool::{
    gpu_vec::GpuVec,
    primitives::block::Vertex,
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
    ops::{
        Deref,
        DerefMut,
    },
};
use voxbrix_common::{
    entity::{
        block::{
            Block,
            Neighbor,
            BLOCKS_IN_CHUNK_EDGE_F32,
        },
        chunk::Chunk,
    },
    math::{
        Vec3F32,
        Vec3I32,
    },
};

const CHUNK_VISIBILITY_RADIUS: f32 = 1.73205077648162841796875f32 * BLOCKS_IN_CHUNK_EDGE_F32;
const CHUNK_CENTER_OFFSET: Vec3F32 = Vec3F32::splat(BLOCKS_IN_CHUNK_EDGE_F32 / 2.0);

pub struct BlkRenderDataChunkComponent(RenderData);

impl BlkRenderDataChunkComponent {
    pub fn new() -> Self {
        Self(RenderData::new())
    }
}

impl Deref for BlkRenderDataChunkComponent {
    type Target = RenderData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BlkRenderDataChunkComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct EnvRenderDataChunkComponent(RenderData);

impl Deref for EnvRenderDataChunkComponent {
    type Target = RenderData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EnvRenderDataChunkComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl EnvRenderDataChunkComponent {
    pub fn new() -> Self {
        Self(RenderData::new())
    }
}

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

pub struct RenderData {
    block_change_queue: VecDeque<Chunk>,
    block_change_neighbors: AHashMap<Chunk, [bool; 6]>,
    chunk_queue: VecDeque<Chunk>,
    enqueued_chunks: AHashSet<Chunk>,
    free_shards: Vec<Vec<Vertex>>,
    prepared_vertex_buffers: AHashMap<Chunk, VertexBuffer>,
    updated_vertex_buffers: AHashMap<Chunk, Vec<Vertex>>,
    free_vertex_buffers: Vec<VertexBuffer>,
}

impl RenderData {
    fn new() -> Self {
        Self {
            block_change_queue: Default::default(),
            block_change_neighbors: Default::default(),
            chunk_queue: Default::default(),
            enqueued_chunks: Default::default(),
            free_shards: Default::default(),
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
        .map(|offset| chunk.checked_add(Vec3I32::from_array(offset)))
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
        if let Some(shard) = self.updated_vertex_buffers.remove(chunk) {
            self.free_shards.push(shard);
        }
        if let Some(vertex_buffer) = self.prepared_vertex_buffers.remove(chunk) {
            self.free_vertex_buffers.push(vertex_buffer);
        }
    }

    pub fn select_chunks(
        &mut self,
        chunk_exists: impl Fn(&Chunk) -> bool,
    ) -> Vec<(Chunk, Vec<Vertex>)> {
        let mut get_shard = |chunk: Chunk| -> (Chunk, Vec<Vertex>) {
            let mut shard = self
                .updated_vertex_buffers
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

                        chunk.checked_add(Vec3I32::from_array(offset))
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

        selected_chunks
    }

    pub fn submit_vertices(
        &mut self,
        par_iter: impl ParallelIterator<Item = (Chunk, Vec<Vertex>)>,
    ) {
        self.updated_vertex_buffers.par_extend(par_iter);
    }

    /// Returns maximum vertices for a single mesh among updated ones.
    pub fn prepare_render(&mut self, renderer: &Renderer) -> u32 {
        let mut max_vertices = 0;

        // Copy queued buffers into GPU:
        // TODO: parallelize?
        let free_shards_iter = self
            .updated_vertex_buffers
            .drain()
            .map(|(chunk, vertices)| {
                let vertex_buffer_byte_size = vertices.len() as u64 * Vertex::size();
                let vertices_len: IndexType = vertices.len().try_into().unwrap();

                if vertices_len == 0 {
                    if let Some(vertex_buffer) = self.prepared_vertex_buffers.remove(&chunk) {
                        self.free_vertex_buffers.push(vertex_buffer);
                    }
                } else {
                    let vertex_buffer =
                        self.prepared_vertex_buffers
                            .entry(chunk)
                            .or_insert_with(|| {
                                self.free_vertex_buffers.pop().unwrap_or_else(|| {
                                    VertexBuffer {
                                        num_vertices: vertices_len,
                                        buffer: GpuVec::new(
                                            renderer.device,
                                            wgpu::BufferUsages::VERTEX,
                                        ),
                                    }
                                })
                            });

                    let mut writer = vertex_buffer.buffer.get_writer(
                        renderer.device,
                        renderer.queue,
                        vertex_buffer_byte_size,
                    );

                    writer.copy_from_slice(bytemuck::cast_slice(&vertices));

                    max_vertices = max_vertices.max(vertices_len);

                    vertex_buffer.num_vertices = vertices_len;
                }

                vertices
            });

        self.free_shards.extend(free_shards_iter);

        max_vertices
    }

    /// For the passed [`is_visible`] function:
    /// first argument is chunk position of the object,
    /// second - chunk offset of the object,
    /// third - radius of the object.
    pub fn get_visible_buffers<'a>(
        &'a self,
        is_visible: impl Fn(Vec3I32, Vec3F32, f32) -> bool + 'a,
    ) -> impl Iterator<Item = (&'a Chunk, &'a VertexBuffer)> + 'a {
        self.prepared_vertex_buffers
            .iter()
            .filter(move |(chunk, _)| {
                is_visible(chunk.position, CHUNK_CENTER_OFFSET, CHUNK_VISIBILITY_RADIUS)
            })
    }
}
