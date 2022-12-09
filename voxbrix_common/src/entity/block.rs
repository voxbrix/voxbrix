use crate::entity::chunk::Chunk;

pub const BLOCKS_IN_CHUNK_EDGE: usize = 16;
pub const BLOCKS_IN_CHUNK_LAYER: usize = BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;
pub const BLOCKS_IN_CHUNK: usize =
    BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;

#[derive(Copy, Clone, Debug)]
pub struct Block(pub usize);

impl Block {
    pub fn to_coords(&self) -> [u8; 3] {
        let z = self.0 / BLOCKS_IN_CHUNK_LAYER;
        let x_y = self.0 % BLOCKS_IN_CHUNK_LAYER;
        let y = x_y / BLOCKS_IN_CHUNK_EDGE;
        let x = x_y % BLOCKS_IN_CHUNK_EDGE;

        [x as u8, y as u8, z as u8]
    }

    pub fn from_coords([x, y, z]: [u8; 3]) -> Self {
        Self(z as usize * BLOCKS_IN_CHUNK_LAYER + y as usize * BLOCKS_IN_CHUNK_EDGE + x as usize)
    }

    pub fn neighbors(&self) -> [Neighbor; 6] {
        let i = self.0 % BLOCKS_IN_CHUNK_EDGE;
        let x_m = if i == 0 {
            let row = self.0 / BLOCKS_IN_CHUNK_EDGE;
            Neighbor::OtherChunk(Block(row + BLOCKS_IN_CHUNK_EDGE - 1))
        } else {
            Neighbor::ThisChunk(Block(self.0 - 1))
        };

        let i = self.0 % BLOCKS_IN_CHUNK_EDGE + 1;
        let x_p = if i >= BLOCKS_IN_CHUNK_EDGE {
            let row = self.0 / BLOCKS_IN_CHUNK_EDGE;
            Neighbor::OtherChunk(Block(row + i - BLOCKS_IN_CHUNK_EDGE))
        } else {
            Neighbor::ThisChunk(Block(self.0 + 1))
        };

        let i = self.0 % BLOCKS_IN_CHUNK_LAYER;
        let y_m = if i < BLOCKS_IN_CHUNK_EDGE {
            let row = self.0 / BLOCKS_IN_CHUNK_LAYER;
            Neighbor::OtherChunk(Block(
                row + BLOCKS_IN_CHUNK_LAYER + i - BLOCKS_IN_CHUNK_EDGE,
            ))
        } else {
            Neighbor::ThisChunk(Block(self.0 - BLOCKS_IN_CHUNK_EDGE))
        };

        let i = self.0 % BLOCKS_IN_CHUNK_LAYER + BLOCKS_IN_CHUNK_EDGE;
        let y_p = if i >= BLOCKS_IN_CHUNK_LAYER {
            let row = self.0 / BLOCKS_IN_CHUNK_LAYER;
            Neighbor::OtherChunk(Block(row + i - BLOCKS_IN_CHUNK_LAYER))
        } else {
            Neighbor::ThisChunk(Block(self.0 + BLOCKS_IN_CHUNK_EDGE))
        };

        let z_m = if self.0 < BLOCKS_IN_CHUNK_LAYER {
            Neighbor::OtherChunk(Block(self.0 + BLOCKS_IN_CHUNK - BLOCKS_IN_CHUNK_LAYER))
        } else {
            Neighbor::ThisChunk(Block(self.0 - BLOCKS_IN_CHUNK_LAYER))
        };

        let i = self.0 + BLOCKS_IN_CHUNK_LAYER;
        let z_p = if i >= BLOCKS_IN_CHUNK {
            Neighbor::OtherChunk(Block(i - BLOCKS_IN_CHUNK))
        } else {
            Neighbor::ThisChunk(Block(i))
        };

        [x_m, x_p, y_m, y_p, z_m, z_p]
    }

    pub fn from_chunk_offset(chunk: Chunk, offset: [i32; 3]) -> (Chunk, Block) {
        const BLOCKS_IN_CHUNK_EDGE_I32: i32 = BLOCKS_IN_CHUNK_EDGE as i32;

        let chunks_blocks = offset.map(|offset| {
            let mut chunk_offset = offset / BLOCKS_IN_CHUNK_EDGE_I32;
            let mut block = offset % BLOCKS_IN_CHUNK_EDGE_I32;

            if block < 0 {
                chunk_offset -= 1;
                block = BLOCKS_IN_CHUNK_EDGE_I32 + block;
            }

            (chunk_offset, block)
        });

        let actual_chunk = Chunk {
            position: [
                chunks_blocks[0].0 + chunk.position[0],
                chunks_blocks[1].0 + chunk.position[1],
                chunks_blocks[2].0 + chunk.position[2],
            ]
            .into(),
            dimension: chunk.dimension,
        };

        let block = Self::from_coords([
            chunks_blocks[0].1 as u8,
            chunks_blocks[1].1 as u8,
            chunks_blocks[2].1 as u8,
        ]);

        (actual_chunk, block)
    }
}

pub enum Neighbor {
    ThisChunk(Block),
    OtherChunk(Block),
}
