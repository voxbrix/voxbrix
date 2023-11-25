use crate::component::chunk::render_priority::{
    self,
    Priority,
    RenderPriorityChunkComponent,
};
use ahash::AHashSet;
use arrayvec::ArrayVec;
use rayon::iter::{
    ParallelDrainRange,
    ParallelExtend,
    ParallelIterator,
};
use std::{
    iter,
    mem,
};
use voxbrix_common::{
    component::{
        block::{
            class::ClassBlockComponent,
            sky_light::{
                SkyLight,
                SkyLightBlockComponent,
            },
            BlocksVec,
        },
        block_class::opacity::OpacityBlockClassComponent,
    },
    entity::chunk::Chunk,
    system::sky_light,
};

pub struct SkyLightSystem {
    processed_chunks: AHashSet<Chunk>,
    chunks_to_compute: AHashSet<Chunk>,
    queue_buffer: Vec<(Chunk, i64, Priority)>,
    pre_compute_buffer: Vec<(Chunk, Option<BlocksVec<SkyLight>>)>,
    post_compute_buffer: Vec<(Chunk, BlocksVec<SkyLight>, ArrayVec<Chunk, 6>)>,
}

impl SkyLightSystem {
    pub fn new() -> Self {
        Self {
            processed_chunks: AHashSet::new(),
            chunks_to_compute: AHashSet::new(),
            queue_buffer: Vec::new(),
            pre_compute_buffer: Vec::new(),
            post_compute_buffer: Vec::new(),
        }
    }

    pub fn fill_chunk_queue(
        &mut self,
        render_priority_cc: &RenderPriorityChunkComponent,
        player_chunk: Chunk,
    ) -> &Vec<(Chunk, i64, Priority)> {
        render_priority::fill_chunk_queue(
            self.chunks_to_compute.iter(),
            &mut self.queue_buffer,
            &render_priority_cc,
            player_chunk,
        );

        &self.queue_buffer
    }

    /// Computes light for the chunk and adds changed neighbors to the queue
    /// to be recalculated.
    pub fn compute_queued(
        &mut self,
        class_bc: &ClassBlockComponent,
        opacity_bcc: &OpacityBlockClassComponent,
        sky_light_bc: &mut SkyLightBlockComponent,
    ) {
        let mut pre_compute_buffer = mem::take(&mut self.pre_compute_buffer);
        let mut post_compute_buffer = mem::take(&mut self.post_compute_buffer);

        let mut chunks_iter = self.queue_buffer.iter();

        let expansion = iter::from_fn(|| {
            let next = chunks_iter.next()?.0;
            self.chunks_to_compute.remove(&next);
            Some(next)
        })
        .filter(|chunk| class_bc.get_chunk(chunk).is_some())
        .map(|chunk| (chunk, sky_light_bc.remove_chunk(&chunk)))
        .take(rayon::current_num_threads());

        pre_compute_buffer.clear();
        pre_compute_buffer.extend(expansion);

        let expansion = pre_compute_buffer
            .par_drain(..)
            .map(|(chunk, old_light_component)| {
                let (light_component, chunks_to_recalc) = sky_light::calc_chunk(
                    chunk,
                    old_light_component,
                    &class_bc,
                    &opacity_bcc,
                    &sky_light_bc,
                );

                (chunk, light_component, chunks_to_recalc)
            });

        post_compute_buffer.clear();
        post_compute_buffer.par_extend(expansion);

        let expansion =
            post_compute_buffer
                .drain(..)
                .flat_map(|(chunk, light_component, chunks_to_recalc)| {
                    sky_light_bc.insert_chunk(chunk, light_component);
                    self.processed_chunks.insert(chunk);
                    chunks_to_recalc.into_iter()
                });

        self.chunks_to_compute.extend(expansion);

        self.pre_compute_buffer = pre_compute_buffer;
        self.post_compute_buffer = post_compute_buffer;
    }

    pub fn add_chunk(&mut self, chunk: Chunk) {
        self.chunks_to_compute.insert(chunk);
    }

    pub fn drain_processed_chunks<'a>(&'a mut self) -> impl Iterator<Item = Chunk> + 'a {
        self.processed_chunks.drain()
    }
}
