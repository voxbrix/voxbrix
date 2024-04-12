pub mod class;

use ahash::{
    AHashMap,
    AHashSet,
};
use nohash_hasher::IntSet;
use voxbrix_common::{
    component::block::{
        BlockComponent,
        BlocksVec,
    },
    entity::{
        block::Block,
        chunk::Chunk,
    },
};

pub struct BlocksVecTracking<'a, T> {
    chunk: Chunk,
    changed_chunks: &'a mut AHashSet<Chunk>,
    changes: &'a mut IntSet<Block>,
    data: &'a mut BlocksVec<T>,
}

impl<T> BlocksVecTracking<'_, T> {
    pub fn set(&mut self, block: Block, value: T) {
        *self.data.get_mut(block) = value;
        self.changes.insert(block);
        self.changed_chunks.insert(self.chunk);
    }
}

struct BlockContainer<C> {
    changes: IntSet<Block>,
    data: C,
}

pub struct TrackingBlockComponent<C> {
    changed_chunks: AHashSet<Chunk>,
    data: AHashMap<Chunk, BlockContainer<C>>,
}

impl<T> BlockComponent<T> for TrackingBlockComponent<BlocksVec<T>> {
    type Blocks = BlocksVec<T>;

    fn get_chunk(&self, chunk: &Chunk) -> Option<&Self::Blocks> {
        self.get_chunk(chunk)
    }
}

pub struct ChangedVecChunk<'a, T> {
    pub chunk: &'a Chunk,
    changes: &'a IntSet<Block>,
    data: &'a BlocksVec<T>,
}

impl<'a, T> ChangedVecChunk<'a, T> {
    pub fn changes(&'a self) -> impl ExactSizeIterator<Item = (&Block, &T)> + 'a {
        self.changes.iter().map(|k| (k, self.data.get(*k)))
    }
}

impl<T> TrackingBlockComponent<T> {
    pub fn new() -> Self {
        Self {
            changed_chunks: AHashSet::new(),
            data: AHashMap::new(),
        }
    }

    /// Inserting the whole chunk is not tracked
    pub fn insert_chunk(&mut self, chunk: Chunk, data: T) {
        self.data.insert(
            chunk,
            BlockContainer {
                changes: IntSet::default(),
                data,
            },
        );
    }

    /// Removing the whole chunk is not tracked
    pub fn remove_chunk(&mut self, chunk: &Chunk) {
        self.changed_chunks.remove(chunk);
        self.data.remove(chunk);
    }
}

impl<T> TrackingBlockComponent<BlocksVec<T>> {
    pub fn get_chunk(&self, chunk: &Chunk) -> Option<&BlocksVec<T>> {
        self.data.get(chunk).as_ref().map(|v| &v.data)
    }

    pub fn get_mut_chunk(&mut self, chunk: &Chunk) -> Option<BlocksVecTracking<T>> {
        let container = self.data.get_mut(chunk)?;

        Some(BlocksVecTracking {
            chunk: *chunk,
            changed_chunks: &mut self.changed_chunks,
            changes: &mut container.changes,
            data: &mut container.data,
        })
    }

    pub fn changed_chunks<'a>(
        &'a self,
    ) -> impl ExactSizeIterator<Item = ChangedVecChunk<'a, T>> + Clone + 'a {
        self.changed_chunks.iter().map(|chunk| {
            let container = self.data.get(&chunk).unwrap();

            ChangedVecChunk {
                chunk,
                changes: &container.changes,
                data: &container.data,
            }
        })
    }

    pub fn clear_changes(&mut self) {
        for chunk in self.changed_chunks.drain() {
            self.data.get_mut(&chunk).unwrap().changes.clear();
        }
    }
}
