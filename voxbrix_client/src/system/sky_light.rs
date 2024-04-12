use crate::{
    component::block::class::ClassBlockComponent,
    system::chunk_render_pipeline::ComputeContext,
};
use arrayvec::ArrayVec;
use rayon::prelude::*;
use std::{
    collections::VecDeque,
    mem,
};
use voxbrix_common::{
    component::{
        block::{
            sky_light::{
                SkyLight,
                SkyLightBlockComponent,
            },
            BlocksVec,
        },
        block_class::opacity::OpacityBlockClassComponent,
    },
    entity::{
        block::Block,
        chunk::Chunk,
    },
    system::sky_light,
};

pub struct SkyLightSystem {
    block_queue_buffers: Vec<VecDeque<Block>>,
    pre_compute_buffer: Vec<(Chunk, Option<BlocksVec<SkyLight>>, VecDeque<Block>)>,
    post_compute_buffer: Vec<(
        Chunk,
        BlocksVec<SkyLight>,
        ArrayVec<Chunk, 6>,
        VecDeque<Block>,
    )>,
}

impl SkyLightSystem {
    pub fn new() -> Self {
        Self {
            block_queue_buffers: Vec::new(),
            pre_compute_buffer: Vec::new(),
            post_compute_buffer: Vec::new(),
        }
    }

    pub fn compute_chunks(
        &mut self,
        mut compute_context: ComputeContext<'_>,
        class_bc: &ClassBlockComponent,
        opacity_bcc: &OpacityBlockClassComponent,
        sky_light_bc: &mut SkyLightBlockComponent,
    ) {
        let mut pre_compute_buffer = mem::take(&mut self.pre_compute_buffer);
        let mut post_compute_buffer = mem::take(&mut self.post_compute_buffer);

        let expansion = compute_context.queue.map(|chunk| {
            (
                chunk,
                sky_light_bc.remove_chunk(&chunk),
                self.block_queue_buffers.pop().unwrap_or(VecDeque::new()),
            )
        });

        pre_compute_buffer.clear();
        pre_compute_buffer.extend(expansion);

        let expansion = pre_compute_buffer.par_drain(..).map(
            |(chunk, old_light_component, mut block_queue_buffer)| {
                let (light_component, chunks_to_recalc) = sky_light::calc_chunk(
                    chunk,
                    &mut block_queue_buffer,
                    old_light_component,
                    class_bc,
                    &opacity_bcc,
                    &sky_light_bc,
                );

                (chunk, light_component, chunks_to_recalc, block_queue_buffer)
            },
        );

        post_compute_buffer.clear();
        post_compute_buffer.par_extend(expansion);

        let expansion = post_compute_buffer.drain(..).flat_map(
            |(chunk, light_component, chunks_to_recalc, block_queue_buffer)| {
                sky_light_bc.insert_chunk(chunk, light_component);
                self.block_queue_buffers.push(block_queue_buffer);
                chunks_to_recalc.into_iter()
            },
        );

        for chunk in expansion {
            compute_context.light_changed(chunk);
        }

        self.post_compute_buffer = post_compute_buffer;
    }
}
