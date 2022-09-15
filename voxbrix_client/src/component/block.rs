use crate::entity::{
    block::Block,
    chunk::Chunk,
};
use std::collections::BTreeMap;

pub mod class;

// #[derive(Serialize, Deserialize, Debug)]
// struct BlockDefinition {
// components: HashMap<String, Value>,
// }
//
// pub trait BlockComponent {
// fn name(&self) -> &str;
// fn from_definition(definition: Value) -> Self;
// }

pub struct Blocks<T> {
    blocks: Vec<T>,
}

impl<T> Blocks<T> {
    pub fn new(blocks: Vec<T>) -> Self {
        Self { blocks }
    }

    pub fn get(&self, i: Block) -> Option<&T> {
        self.blocks.get(i.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Block, &T)> {
        self.blocks.iter().enumerate().map(|(i, b)| (Block(i), b))
    }
}

pub struct BlockComponent<T> {
    chunks: BTreeMap<Chunk, Blocks<T>>,
}

impl<T> BlockComponent<T> {
    pub fn new() -> Self {
        Self {
            chunks: BTreeMap::new(),
        }
    }

    pub fn get_chunk(&self, chunk: &Chunk) -> Option<&Blocks<T>> {
        self.chunks.get(&chunk)
    }

    pub fn insert_chunk(&mut self, chunk: Chunk, blocks: Blocks<T>) {
        self.chunks.insert(chunk, blocks);
    }

    pub fn remove_chunks<F>(&mut self, mut remove_fn: F)
    where
        F: FnMut(&Chunk) -> bool,
    {
        self.chunks.retain(|c, _| !remove_fn(c));
    }
}
