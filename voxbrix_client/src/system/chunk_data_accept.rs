use crate::component::{
    block::{
        class::ClassBlockComponent,
        environment::EnvironmentBlockComponent,
        metadata::MetadataBlockComponent,
    },
    chunk::sky_light_data::SkyLightDataChunkComponent,
};
use voxbrix_common::{
    component::chunk::status::{
        ChunkStatus,
        StatusChunkComponent,
    },
    ChunkData,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ChunkDataAcceptSystem;

impl System for ChunkDataAcceptSystem {
    type Data<'a> = ChunkDataAcceptSystemData<'a>;
}

#[derive(SystemData)]
pub struct ChunkDataAcceptSystemData<'a> {
    class_bc: &'a mut ClassBlockComponent,
    environment_bc: &'a mut EnvironmentBlockComponent,
    metadata_bc: &'a mut MetadataBlockComponent,
    status_cc: &'a mut StatusChunkComponent,
    sky_light_data_cc: &'a mut SkyLightDataChunkComponent,
}

impl ChunkDataAcceptSystemData<'_> {
    pub fn run(&mut self, chunk_data_set: Vec<ChunkData>) {
        for chunk_data in chunk_data_set {
            let ChunkData {
                chunk,
                block_classes,
                block_environment,
                block_metadata,
            } = chunk_data;

            self.class_bc.insert_chunk(chunk, block_classes);
            self.environment_bc.insert_chunk(chunk, block_environment);
            self.metadata_bc.insert_chunk(chunk, block_metadata);
            self.status_cc.insert(chunk, ChunkStatus::Active);

            self.sky_light_data_cc.enqueue_chunk(chunk);
        }
    }
}
