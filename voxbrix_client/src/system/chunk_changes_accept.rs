use crate::component::{
    block::{
        class::ClassBlockComponent,
        environment::EnvironmentBlockComponent,
        metadata::MetadataBlockComponent,
    },
    chunk::{
        render_data::RenderDataChunkComponent,
        sky_light_data::SkyLightDataChunkComponent,
    },
};
use log::error;
use serde::de::DeserializeOwned;
use voxbrix_common::{
    component::block::{
        metadata::BlockMetadata,
        BlockComponentSimple,
        BlocksVec,
    },
    entity::{
        block_class::BlockClass,
        block_environment::BlockEnvironment,
    },
    messages::client::ChunkChanges,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub enum Error {
    DecodeError,
}

pub struct ChunkChangesAcceptSystem;

impl System for ChunkChangesAcceptSystem {
    type Data<'a> = ChunkChangesAcceptSystemData<'a>;
}

#[derive(SystemData)]
pub struct ChunkChangesAcceptSystemData<'a> {
    class_bc: &'a mut ClassBlockComponent,
    environment_bc: &'a mut EnvironmentBlockComponent,
    metadata_bc: &'a mut MetadataBlockComponent,
    sky_light_data_cc: &'a mut SkyLightDataChunkComponent,
    render_data_cc: &'a mut RenderDataChunkComponent,
}

fn run_inner<T>(
    changes: ChunkChanges<'_, T>,
    component: &mut BlockComponentSimple<BlocksVec<T>>,
    sky_light_data_cc: &mut SkyLightDataChunkComponent,
    render_data_cc: &mut RenderDataChunkComponent,
) -> Result<(), Error>
where
    T: DeserializeOwned,
{
    let mut chunk_decoder = changes.decode_chunks().map_err(|_| {
        error!("unable to decode chunk changes");
        Error::DecodeError
    })?;

    while let Some(chunk_change) = chunk_decoder.decode_chunk() {
        let mut chunk_change = chunk_change.map_err(|_| {
            error!("unable to decode chunk change");
            Error::DecodeError
        })?;

        let chunk = chunk_change.chunk();

        let mut component = component.get_mut_chunk(&chunk);

        while let Some(block_change) = chunk_change.decode_block() {
            let (block, block_class) = block_change.map_err(|_| {
                error!("unable to decode block changes");
                Error::DecodeError
            })?;

            if let Some(ref mut component) = component {
                *component.get_mut(block) = block_class;
                sky_light_data_cc.block_change(&chunk, block);
                render_data_cc.block_change(&chunk, block);
            }
        }
    }

    Ok(())
}

impl ChunkChangesAcceptSystemData<'_> {
    pub fn run(
        &mut self,
        block_class: ChunkChanges<'_, BlockClass>,
        block_environment: ChunkChanges<'_, BlockEnvironment>,
        block_metadata: ChunkChanges<'_, BlockMetadata>,
    ) -> Result<(), Error> {
        run_inner(
            block_class,
            &mut self.class_bc,
            &mut self.sky_light_data_cc,
            &mut self.render_data_cc,
        )?;
        run_inner(
            block_environment,
            &mut self.environment_bc,
            &mut self.sky_light_data_cc,
            &mut self.render_data_cc,
        )?;
        run_inner(
            block_metadata,
            &mut self.metadata_bc,
            &mut self.sky_light_data_cc,
            &mut self.render_data_cc,
        )?;

        Ok(())
    }
}
