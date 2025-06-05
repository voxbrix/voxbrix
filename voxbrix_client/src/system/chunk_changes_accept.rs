use crate::component::{
    block::class::ClassBlockComponent,
    chunk::{
        render_data::RenderDataChunkComponent,
        sky_light_data::SkyLightDataChunkComponent,
    },
};
use log::error;
use voxbrix_common::messages::client::ChunkChanges;
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
    sky_light_data_cc: &'a mut SkyLightDataChunkComponent,
    render_data_cc: &'a mut RenderDataChunkComponent,
}

impl ChunkChangesAcceptSystemData<'_> {
    pub fn run(&mut self, changes: ChunkChanges<'_>) -> Result<(), Error> {
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

            let mut chunk_classes = self.class_bc.get_mut_chunk(&chunk);

            while let Some(block_change) = chunk_change.decode_block() {
                let (block, block_class) = block_change.map_err(|_| {
                    error!("unable to decode block changes");
                    Error::DecodeError
                })?;

                if let Some(ref mut chunk_classes) = chunk_classes {
                    *chunk_classes.get_mut(block) = block_class;
                    self.sky_light_data_cc.block_change(&chunk, block);
                    self.render_data_cc.block_change(&chunk, block);
                }
            }
        }

        Ok(())
    }
}
