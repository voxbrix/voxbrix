use crate::{
    entity::{
        block::{
            Block,
            BLOCKS_IN_CHUNK,
        },
        chunk::Chunk,
    },
    pack::Pack,
};
use rayon::prelude::*;
use serde::{
    de::{
        Deserialize,
        Deserializer,
        Error as _,
        SeqAccess,
        Visitor,
    },
    ser::{
        Serialize,
        SerializeTuple,
        Serializer,
    },
};
use std::{
    collections::HashMap,
    fmt,
};

pub mod metadata;
pub mod sky_light;

pub trait BlockComponent<T> {
    type Blocks: Blocks<T>;

    fn get_chunk(&self, chunk: &Chunk) -> Option<&Self::Blocks>;
}

pub trait Blocks<T> {
    fn get(&self, block: Block) -> &T;
}

pub struct BlocksVecBuilder<T>(Vec<T>);

impl<T> BlocksVecBuilder<T> {
    pub fn new() -> Self {
        let mut vec = Vec::new();
        vec.reserve_exact(BLOCKS_IN_CHUNK);
        Self(vec)
    }

    pub fn push(&mut self, value: T) {
        self.0.push(value)
    }

    pub fn build(self) -> BlocksVec<T> {
        BlocksVec(
            self.0
                .into_boxed_slice()
                .try_into()
                .ok()
                .expect("BlocksVecBuilder is not complete"),
        )
    }
}

impl<'de, T> Visitor<'de> for BlocksVecBuilder<T>
where
    T: Deserialize<'de>,
{
    type Value = BlocksVec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "an array of {} elements", BLOCKS_IN_CHUNK)
    }

    fn visit_seq<A>(mut self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        for _ in 0 .. BLOCKS_IN_CHUNK {
            let value = seq
                .next_element()?
                .ok_or(A::Error::custom("not enough Blocks for Chunk"))?;

            self.push(value);
        }

        Ok(self.build())
    }
}

#[derive(Clone, Debug)]
pub struct BlocksVec<T>(Box<[T; BLOCKS_IN_CHUNK]>);

impl<T> Serialize for BlocksVec<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut tup = serializer.serialize_tuple(BLOCKS_IN_CHUNK)?;

        for e in self.0.iter() {
            tup.serialize_element(e)?;
        }

        tup.end()
    }
}
impl<'de, T> Deserialize<'de> for BlocksVec<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_tuple(BLOCKS_IN_CHUNK, BlocksVecBuilder::new())
    }
}

impl<T> Blocks<T> for BlocksVec<T> {
    fn get(&self, block: Block) -> &T {
        self.get(block)
    }
}

impl<T> BlocksVec<T> {
    pub fn new() -> BlocksVecBuilder<T> {
        BlocksVecBuilder::new()
    }

    pub fn get(&self, block: Block) -> &T {
        self.0.get(block.as_usize()).unwrap()
    }

    pub fn get_mut(&mut self, block: Block) -> &mut T {
        self.0.get_mut(block.as_usize()).unwrap()
    }

    pub fn iter<'a>(&'a self) -> impl ExactSizeIterator<Item = (Block, &'a T)> + 'a {
        self.0
            .iter()
            .enumerate()
            .map(|(b, v)| (Block::from_usize(b).unwrap(), v))
    }
}

impl<'a, T> BlocksVec<T>
where
    T: Send + Sync + 'a,
{
    pub fn par_iter(&'a self) -> impl IndexedParallelIterator<Item = (Block, &'a T)> + 'a {
        self.0
            .as_slice()
            .par_iter()
            .enumerate()
            .map(|(b, v)| (Block::from_usize(b).unwrap(), v))
    }
}

impl<T> BlocksVec<T>
where
    T: Clone,
{
    pub fn new_cloned(value: T) -> Self {
        let mut blocks = Self::new();

        for _ in 0 .. BLOCKS_IN_CHUNK {
            blocks.push(value.clone());
        }

        blocks.build()
    }
}

impl<T> Pack for BlocksVec<T> {
    const DEFAULT_COMPRESSED: bool = true;
}

pub struct BlockComponentSimple<C> {
    chunks: HashMap<Chunk, C>,
}

impl<T> BlockComponent<T> for BlockComponentSimple<BlocksVec<T>> {
    type Blocks = BlocksVec<T>;

    fn get_chunk(&self, chunk: &Chunk) -> Option<&Self::Blocks> {
        self.get_chunk(chunk)
    }
}

impl<T> BlockComponentSimple<T> {
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
