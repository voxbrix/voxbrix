use crate::entity::{
    block::{
        Block,
        BLOCKS_IN_CHUNK,
        BLOCKS_IN_CHUNK_EDGE,
    },
    chunk::Chunk,
};
use serde::{
    Deserialize,
    Serialize,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(bound(
    serialize = "for<'a> T: serde::Serialize + serde::Deserialize<'a>",
    deserialize = "T: serde::Serialize + serde::Deserialize<'de>"
))]
pub struct Blocks<T> {
    #[serde(with = "serde_big_array::BigArray")]
    blocks: [T; BLOCKS_IN_CHUNK],
}

impl<T> Blocks<T> {
    pub fn new(blocks: [T; BLOCKS_IN_CHUNK]) -> Self {
        Self { blocks }
    }

    pub fn get(&self, i: Block) -> Option<&T> {
        self.blocks.get(i.0)
    }

    pub fn get_mut(&mut self, i: Block) -> Option<&mut T> {
        self.blocks.get_mut(i.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Block, &T)> {
        self.blocks.iter().enumerate().map(|(i, b)| (Block(i), b))
    }

    pub fn iter_with_coords(&self) -> impl Iterator<Item = (Block, [usize; 3], &T)> {
        let coords = (0 .. BLOCKS_IN_CHUNK_EDGE).flat_map(move |z| {
            (0 .. BLOCKS_IN_CHUNK_EDGE)
                .flat_map(move |y| (0 .. BLOCKS_IN_CHUNK_EDGE).map(move |x| ([x, y, z])))
        });
        self.blocks
            .iter()
            .enumerate()
            .zip(coords)
            .map(|((i, b), c)| (Block(i), c, b))
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

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn get_chunk(&self, chunk: &Chunk) -> Option<&Blocks<T>> {
        self.chunks.get(&chunk)
    }

    pub fn get_mut_chunk(&mut self, chunk: &Chunk) -> Option<&mut Blocks<T>> {
        self.chunks.get_mut(&chunk)
    }

    pub fn insert_chunk(&mut self, chunk: Chunk, blocks: Blocks<T>) {
        self.chunks.insert(chunk, blocks);
    }

    pub fn remove_chunk(&mut self, chunk: &Chunk) {
        self.chunks.remove(chunk);
    }

    pub fn retain<F>(&mut self, mut retain_fn: F)
    where
        F: FnMut(&Chunk) -> bool,
    {
        self.chunks.retain(|c, _| retain_fn(c));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Chunk, &Blocks<T>)> {
        self.chunks.iter()
    }
}
