use crate::entity::chunk::Chunk;
use std::collections::BTreeMap;

pub mod status;

pub struct ChunkComponent<T> {
    chunks: BTreeMap<Chunk, T>,
}

impl<T> ChunkComponent<T> {
    pub fn new() -> Self {
        Self {
            chunks: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn get_chunk(&self, chunk: &Chunk) -> Option<&T> {
        self.chunks.get(&chunk)
    }

    pub fn insert_chunk(&mut self, chunk: Chunk, value: T) {
        self.chunks.insert(chunk, value);
    }

    pub fn remove_chunk(&mut self, chunk: &Chunk) {
        self.chunks.remove(chunk);
    }

    pub fn filter_out_chunks<F>(&mut self, mut remove_fn: F)
    where
        F: FnMut(&Chunk) -> bool,
    {
        self.chunks.retain(|c, _| !remove_fn(c));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Chunk, &T)> {
        self.chunks.iter()
    }
}
