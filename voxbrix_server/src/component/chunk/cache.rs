use crate::component::chunk::ChunkComponent;
use std::rc::Rc;

pub type CacheChunkComponent = ChunkComponent<Rc<Vec<u8>>>;
