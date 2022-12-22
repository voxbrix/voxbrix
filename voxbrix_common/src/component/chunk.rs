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

    pub fn get(&self, chunk: &Chunk) -> Option<&T> {
        self.chunks.get(&chunk)
    }

    pub fn get_mut(&mut self, chunk: &Chunk) -> Option<&mut T> {
        self.chunks.get_mut(&chunk)
    }

    pub fn insert(&mut self, chunk: Chunk, value: T) -> Option<T> {
        self.chunks.insert(chunk, value)
    }

    pub fn remove(&mut self, chunk: &Chunk) -> Option<T> {
        self.chunks.remove(chunk)
    }

    pub fn retain<F>(&mut self, retain_fn: F)
    where
        F: FnMut(&Chunk, &mut T) -> bool,
    {
        self.chunks.retain(retain_fn);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Chunk, &T)> {
        self.chunks.iter()
    }
}
