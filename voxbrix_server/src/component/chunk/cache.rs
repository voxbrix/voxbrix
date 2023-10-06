use std::rc::Rc;
use voxbrix_common::component::chunk::ChunkComponent;

/// Compressed, ready-to-be-send ClientAccept with ChunkData.
pub type CacheChunkComponent = ChunkComponent<Rc<Vec<u8>>>;
