use std::sync::Arc;
use voxbrix_common::component::chunk::ChunkComponent;

/// Compressed, ready-to-be-send ClientAccept with ChunkData.
#[derive(Clone)]
pub struct ChunkCache(Arc<Vec<u8>>);

impl ChunkCache {
    pub fn new(data: Vec<u8>) -> Self {
        Self(Arc::new(data))
    }

    pub fn into_inner(self) -> Arc<Vec<u8>> {
        self.0
    }
}

impl From<Arc<Vec<u8>>> for ChunkCache {
    fn from(value: Arc<Vec<u8>>) -> Self {
        Self(value)
    }
}

pub type CacheChunkComponent = ChunkComponent<ChunkCache>;
