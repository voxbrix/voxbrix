use std::sync::Arc;
use voxbrix_common::ChunkData;

pub enum SharedEvent {
    ChunkLoaded {
        data: ChunkData,
        data_encoded: Arc<[u8]>,
    },
}
