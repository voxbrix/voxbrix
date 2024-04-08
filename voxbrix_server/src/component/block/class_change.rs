use ahash::{
    AHashMap,
    AHashSet,
};
use nohash_hasher::IntMap;
use voxbrix_common::entity::{
    block::Block,
    block_class::BlockClass,
    chunk::Chunk,
};

pub struct BlockClassChanges<'a> {
    chunk: Chunk,
    changes: &'a mut AHashSet<Chunk>,
    data: &'a mut IntMap<Block, BlockClass>,
}

impl BlockClassChanges<'_> {
    pub fn change(&mut self, block: Block, class: BlockClass) {
        self.data.insert(block, class);
        self.changes.insert(self.chunk);
    }

    pub fn drain_changes<'a>(
        &'a mut self,
    ) -> impl ExactSizeIterator<Item = (Block, BlockClass)> + 'a {
        self.data.drain()
    }
}

pub struct ClassChangeBlockComponent {
    changes: AHashSet<Chunk>,
    data: AHashMap<Chunk, IntMap<Block, BlockClass>>,
}

pub struct ChangedChunk<'a> {
    pub chunk: &'a Chunk,
    changes: &'a IntMap<Block, BlockClass>,
}

impl<'a> ChangedChunk<'a> {
    pub fn changes(&'a self) -> impl ExactSizeIterator<Item = (Block, BlockClass)> + 'a {
        self.changes.iter().map(|(k, v)| (*k, *v))
    }
}

impl ClassChangeBlockComponent {
    pub fn new() -> Self {
        Self {
            changes: AHashSet::new(),
            data: AHashMap::new(),
        }
    }

    pub fn insert_chunk(&mut self, chunk: Chunk) {
        self.data.insert(chunk, IntMap::default());
    }

    pub fn remove_chunk(&mut self, chunk: &Chunk) {
        self.changes.remove(chunk);
        self.data.remove(chunk);
    }

    pub fn get_mut_chunk(&mut self, chunk: &Chunk) -> Option<BlockClassChanges> {
        Some(BlockClassChanges {
            chunk: *chunk,
            changes: &mut self.changes,
            data: self.data.get_mut(chunk)?,
        })
    }

    pub fn changed_chunks<'a>(
        &'a mut self,
    ) -> impl ExactSizeIterator<Item = ChangedChunk<'a>> + Clone + 'a {
        self.changes.iter().map(|chunk| {
            ChangedChunk {
                chunk,
                changes: self.data.get(&chunk).unwrap(),
            }
        })
    }

    pub fn clear(&mut self) {
        self.changes
            .drain()
            .map(|chunk| self.data.get_mut(&chunk).unwrap().clear());
    }
}
