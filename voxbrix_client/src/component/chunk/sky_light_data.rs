use ahash::AHashMap;
pub use queues::BlockQueue;
use queues::ChunkQueue;
use voxbrix_common::entity::{
    block::Block,
    chunk::Chunk,
};

pub struct SkyLightDataChunkComponent {
    chunk_queue: ChunkQueue,
    block_queues: AHashMap<Chunk, BlockQueue>,
}

impl SkyLightDataChunkComponent {
    pub fn new() -> Self {
        Self {
            chunk_queue: ChunkQueue::new(),
            block_queues: AHashMap::new(),
        }
    }

    pub fn is_queue_empty(&self) -> bool {
        self.chunk_queue.is_empty()
    }

    pub fn enqueue_chunk(&mut self, chunk: Chunk) {
        self.chunk_queue.push(chunk);
    }

    pub fn block_change(&mut self, chunk: &Chunk, block: Block) {
        if let Some(queue) = self.block_queues.get_mut(&chunk) {
            queue.push_this_chunk(block);
            self.enqueue_chunk(*chunk);
        }
    }

    pub fn remove_chunk(&mut self, chunk: &Chunk) {
        self.chunk_queue.remove(chunk);
        self.block_queues.remove(chunk);
    }

    pub fn get_block_queue_mut(&mut self, chunk: &Chunk) -> Option<&mut BlockQueue> {
        self.block_queues.get_mut(chunk)
    }

    pub fn insert_block_queue(&mut self, chunk: Chunk, block_queue: BlockQueue) {
        self.block_queues.insert(chunk, block_queue);
    }

    pub fn drain_chunk_queue<'a>(
        &'a mut self,
    ) -> impl Iterator<Item = (Chunk, Option<BlockQueue>)> + 'a {
        self.chunk_queue.get_queue().map(|chunk| {
            let block_queue = self.block_queues.remove(&chunk);

            (chunk, block_queue)
        })
    }
}

mod queues {
    use ahash::AHashSet;
    use std::{
        collections::VecDeque,
        iter,
    };
    use voxbrix_common::entity::{
        block::{
            Block,
            BLOCKS_IN_CHUNK,
            BLOCKS_IN_CHUNK_EDGE,
        },
        chunk::Chunk,
    };

    pub struct BlockQueue {
        current_position: usize,
        this_chunk: Box<[bool; BLOCKS_IN_CHUNK]>,
        other_chunks: Box<[bool; BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE * 6]>,
    }

    impl BlockQueue {
        pub fn new_full(other_chunks_fill: bool) -> Self {
            Self {
                current_position: BLOCKS_IN_CHUNK - 1,
                this_chunk: Box::new([true; BLOCKS_IN_CHUNK]),
                other_chunks: Box::new(
                    [other_chunks_fill; BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE * 6],
                ),
            }
        }

        pub fn push_this_chunk(&mut self, block: Block) {
            self.this_chunk[block.as_usize()] = true;
        }

        pub fn push_other_chunk(&mut self, side: usize, block: Block) {
            let fixed_axis = side / 2;
            let coords = block.into_coords();

            let (a0, a1) = match fixed_axis {
                0 => (1, 2),
                1 => (0, 2),
                2 => (0, 1),
                _ => unreachable!(),
            };

            let index = coords[a0]
                + coords[a1] * BLOCKS_IN_CHUNK_EDGE
                + side * BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;
            self.other_chunks[index] = true;
        }

        pub fn pop(&mut self) -> Option<Block> {
            let mut counter = 0;
            while self.this_chunk[self.current_position] == false && counter < BLOCKS_IN_CHUNK {
                // For faster calculation of new chunks, calculation must be started
                // with blocks in queue ordered in sky -> ground direction
                self.current_position = self
                    .current_position
                    .checked_sub(1)
                    .unwrap_or(BLOCKS_IN_CHUNK - 1);
                counter += 1;
            }

            let is_some = self.this_chunk[self.current_position];

            self.this_chunk[self.current_position] = false;

            is_some.then_some(Block::from_usize(self.current_position).unwrap())
        }

        pub fn drain_other_chunk_on_side<'a>(
            &'a mut self,
            side: usize,
        ) -> impl Iterator<Item = Block> + 'a {
            let fixed_axis = side / 2;

            let (a0, a1) = match fixed_axis {
                0 => (1, 2),
                1 => (0, 2),
                2 => (0, 1),
                _ => unreachable!(),
            };

            // +1 here because we need neighbor's block not this chunk's
            let fixed_axis_value = ((side + 1) % 2) * (BLOCKS_IN_CHUNK_EDGE - 1);

            (0 .. BLOCKS_IN_CHUNK_EDGE)
                .flat_map(|a1val| (0 .. BLOCKS_IN_CHUNK_EDGE).map(move |a0val| (a0val, a1val)))
                .filter_map(move |(a0val, a1val)| {
                    let index = a0val
                        + a1val * BLOCKS_IN_CHUNK_EDGE
                        + side * BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;

                    if self.other_chunks[index] {
                        self.other_chunks[index] = false;

                        let mut coords = [0; 3];

                        coords[a0] = a0val;
                        coords[a1] = a1val;
                        coords[fixed_axis] = fixed_axis_value;

                        Some(Block::from_coords(coords))
                    } else {
                        None
                    }
                })
        }

        pub fn is_empty(&self) -> bool {
            self.this_chunk.iter().find(|b| **b).is_none()
        }
    }

    pub struct ChunkQueue {
        // We need to take turns with even/odd chunks because to parallelize the process
        // we remove the chunks being processed from the sky light component.
        // With 2 queues we still have neighbor chunks guaranteed to be readable from the component because
        // the neighbor chunks for odd chunks will be even chunks and vice versa.
        even_chunk_queue: VecDeque<Chunk>,
        odd_chunk_queue: VecDeque<Chunk>,
        enqueued_chunks: AHashSet<Chunk>,
        is_even_next: bool,
    }

    impl ChunkQueue {
        pub fn new() -> Self {
            Self {
                even_chunk_queue: VecDeque::new(),
                odd_chunk_queue: VecDeque::new(),
                enqueued_chunks: AHashSet::new(),
                is_even_next: true,
            }
        }

        pub fn is_empty(&self) -> bool {
            self.enqueued_chunks.is_empty()
        }

        pub fn push(&mut self, chunk: Chunk) {
            if !self.enqueued_chunks.insert(chunk) {
                return;
            }

            if chunk.position.into_iter().map(|i| i as i64).sum::<i64>() % 2 == 0 {
                self.even_chunk_queue.push_back(chunk);
            } else {
                self.odd_chunk_queue.push_back(chunk);
            }
        }

        pub fn remove(&mut self, chunk: &Chunk) -> bool {
            self.enqueued_chunks.remove(chunk)
        }

        pub fn get_queue<'a>(&'a mut self) -> impl Iterator<Item = Chunk> + 'a {
            let queue = if self.is_even_next {
                self.is_even_next = false;
                &mut self.even_chunk_queue
            } else {
                self.is_even_next = true;
                &mut self.odd_chunk_queue
            };

            iter::from_fn(|| {
                let chunk = queue.pop_front()?;

                // Lazily ignoring already removed chunks
                if !self.enqueued_chunks.remove(&chunk) {
                    return None;
                }

                Some(chunk)
            })
        }
    }
}
