use crate::system::chunk_render_pipeline::ComputeContext;
use arrayvec::ArrayVec;
use rayon::prelude::*;
use std::mem;
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
    pre_compute_buffer: Vec<(Chunk, Option<BlocksVec<SkyLight>>)>,
    post_compute_buffer: Vec<(Chunk, BlocksVec<SkyLight>, ArrayVec<Chunk, 6>)>,
}

impl SkyLightSystem {
    pub fn new() -> Self {
        Self {
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

        let expansion = compute_context
            .queue
            .map(|chunk| (chunk, sky_light_bc.remove_chunk(&chunk)));

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
                    chunks_to_recalc.into_iter()
                });

        for chunk in expansion {
            compute_context.light_changed(chunk);
        }

        self.post_compute_buffer = post_compute_buffer;
    }
}
