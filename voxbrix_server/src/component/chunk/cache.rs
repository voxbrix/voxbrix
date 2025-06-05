use std::sync::Arc;
use voxbrix_common::component::chunk::ChunkComponent;

/// Compressed, ready-to-be-send ClientAccept with ChunkData.
#[derive(Clone)]
pub struct ChunkCache(Arc<[u8]>);

impl ChunkCache {
    pub fn new(data: Vec<u8>) -> Self {
        Self(data.into())
    }

    pub fn into_inner(self) -> Arc<[u8]> {
        self.0
    }
}

impl From<Arc<[u8]>> for ChunkCache {
    fn from(value: Arc<[u8]>) -> Self {
        Self(value)
    }
}

pub type CacheChunkComponent = ChunkComponent<ChunkCache>;
