use std::rc::Rc;
use voxbrix_common::component::chunk::ChunkComponent;

pub type CacheChunkComponent = ChunkComponent<Rc<Vec<u8>>>;
