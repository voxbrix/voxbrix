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
use bincode::{
    de::{
        BorrowDecoder,
        Decoder,
    },
    error::DecodeError,
    BorrowDecode,
    Decode,
    Encode,
};
use std::{
    alloc::{
        self,
        Layout,
    },
    collections::HashMap,
};

pub mod sky_light;

pub trait BlockComponent<T> {
    type Blocks: Blocks<T>;

    fn get_chunk(&self, chunk: &Chunk) -> Option<&Self::Blocks>;
}

pub trait Blocks<T> {
    fn get(&self, block: Block) -> &T;
}

pub struct BlocksVecBuilder<T> {
    next: usize,
    uninit: Box<[T; BLOCKS_IN_CHUNK]>,
}

impl<T> BlocksVecBuilder<T> {
    pub fn new() -> Self {
        // SAFETY: fast and safe way to get Box of [0u8; MAX_PACKET_SIZE]
        // without copying stack to heap (as would be with Box::new())
        // https://doc.rust-lang.org/std/boxed/index.html#memory-layout
        unsafe {
            let layout = Layout::new::<[T; BLOCKS_IN_CHUNK]>();
            let ptr = alloc::alloc(layout);
            if ptr.is_null() {
                alloc::handle_alloc_error(layout);
            }

            Self {
                next: 0,
                uninit: Box::from_raw(ptr.cast()),
            }
        }
    }

    pub fn push(&mut self, value: T) {
        self.uninit[self.next] = value;
        self.next += 1;
    }

    pub fn build(self) -> BlocksVec<T> {
        if self.next != BLOCKS_IN_CHUNK {
            panic!("BlocksVecBuilder is not complete");
        }

        BlocksVec(self.uninit)
    }
}

#[derive(Encode, Clone, Debug)]
pub struct BlocksVec<T>(Box<[T; BLOCKS_IN_CHUNK]>);

impl<T> Decode for BlocksVec<T>
where
    T: Decode,
{
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        let mut value = Self::new();

        for _ in 0 .. BLOCKS_IN_CHUNK {
            value.push(Decode::decode(decoder)?);
        }

        Ok(value.build())
    }
}

impl<'de, T> BorrowDecode<'de> for BlocksVec<T>
where
    T: BorrowDecode<'de>,
{
    fn borrow_decode<D: BorrowDecoder<'de>>(decoder: &mut D) -> Result<Self, DecodeError> {
        let mut value = Self::new();

        for _ in 0 .. BLOCKS_IN_CHUNK {
            value.push(BorrowDecode::borrow_decode(decoder)?);
        }

        Ok(value.build())
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

    pub fn iter<'a>(&'a self) -> impl ExactSizeIterator<Item = (Block, &T)> + 'a {
        self.0
            .iter()
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
