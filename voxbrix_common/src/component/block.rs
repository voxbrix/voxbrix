use crate::{
    entity::{
        block::Block,
        chunk::Chunk,
    },
    pack::Pack,
};
use bincode::{
    Decode,
    Encode,
};
use nohash_hasher::IntMap;
use std::collections::HashMap;

pub mod class;
pub mod sky_light;

#[derive(Encode, Decode, Clone, Debug)]
pub struct BlocksVec<T> {
    blocks: Vec<T>,
}

impl<T> BlocksVec<T> {
    pub fn new(blocks: Vec<T>) -> Self {
        Self { blocks }
    }

    pub fn get(&self, block: Block) -> &T {
        self.blocks.get(block.0).unwrap()
    }

    pub fn get_mut(&mut self, block: Block) -> &mut T {
        self.blocks.get_mut(block.0).unwrap()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Block, &T)> {
        self.blocks.iter().zip(0 ..).map(|(v, b)| (Block(b), v))
    }
}

impl<T> Pack for BlocksVec<T> {
    const DEFAULT_COMPRESSED: bool = true;
}

#[derive(Encode, Decode, Clone, Debug)]
pub struct BlocksMap<T> {
    blocks: IntMap<Block, T>,
}

impl<T> BlocksMap<T> {
    pub fn new(blocks: IntMap<Block, T>) -> Self {
        Self { blocks }
    }

    pub fn get(&self, i: Block) -> Option<&T> {
        self.blocks.get(&i)
    }

    pub fn get_mut(&mut self, i: Block) -> Option<&mut T> {
        self.blocks.get_mut(&i)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Block, &T)> {
        self.blocks.iter().map(|(i, t)| (*i, t))
    }
}

pub struct BlockComponent<T> {
    chunks: HashMap<Chunk, T>,
}

impl<T> BlockComponent<T> {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn get_chunk(&self, chunk: &Chunk) -> Option<&T> {
        self.chunks.get(chunk)
    }

    pub fn get_mut_chunk(&mut self, chunk: &Chunk) -> Option<&mut T> {
        self.chunks.get_mut(chunk)
    }

    pub fn insert_chunk(&mut self, chunk: Chunk, blocks: T) {
        self.chunks.insert(chunk, blocks);
    }

    pub fn remove_chunk(&mut self, chunk: &Chunk) -> Option<T> {
        self.chunks.remove(chunk)
    }

    pub fn retain<F>(&mut self, mut retain_fn: F)
    where
        F: FnMut(&Chunk) -> bool,
    {
        self.chunks.retain(|c, _| retain_fn(c));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Chunk, &T)> {
        self.chunks.iter()
    }
}
