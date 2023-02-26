use crate::{
    entity::chunk::Chunk,
    math::Vec3,
};
use serde::{
    Deserialize,
    Serialize,
};

pub const BLOCKS_IN_CHUNK_EDGE: usize = 32;
pub const BLOCKS_IN_CHUNK_LAYER: usize = BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;
pub const BLOCKS_IN_CHUNK: usize =
    BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Block(pub usize);

impl Block {
    pub fn to_coords(&self) -> [usize; 3] {
        let z = self.0 / BLOCKS_IN_CHUNK_LAYER;
        let x_y = self.0 % BLOCKS_IN_CHUNK_LAYER;
        let y = x_y / BLOCKS_IN_CHUNK_EDGE;
        let x = x_y % BLOCKS_IN_CHUNK_EDGE;

        [x, y, z]
    }

    pub fn from_coords([x, y, z]: [usize; 3]) -> Self {
        Self(z * BLOCKS_IN_CHUNK_LAYER + y * BLOCKS_IN_CHUNK_EDGE + x)
    }

    pub fn neighbors_in_coords(&self, [x, y, z]: [usize; 3]) -> [Neighbor; 6] {
        let x_m = if x == 0 {
            Neighbor::OtherChunk(Block(self.0 + BLOCKS_IN_CHUNK_EDGE - 1))
        } else {
            Neighbor::ThisChunk(Block(self.0 - 1))
        };

        let x_p = if x + 1 < BLOCKS_IN_CHUNK_EDGE {
            Neighbor::ThisChunk(Block(self.0 + 1))
        } else {
            Neighbor::OtherChunk(Block(self.0 + 1 - BLOCKS_IN_CHUNK_EDGE))
        };

        let y_m = if y == 0 {
            Neighbor::OtherChunk(Block(self.0 + BLOCKS_IN_CHUNK_LAYER - BLOCKS_IN_CHUNK_EDGE))
        } else {
            Neighbor::ThisChunk(Block(self.0 - BLOCKS_IN_CHUNK_EDGE))
        };

        let y_p = if y + 1 < BLOCKS_IN_CHUNK_EDGE {
            Neighbor::ThisChunk(Block(self.0 + BLOCKS_IN_CHUNK_EDGE))
        } else {
            Neighbor::OtherChunk(Block(self.0 + BLOCKS_IN_CHUNK_EDGE - BLOCKS_IN_CHUNK_LAYER))
        };

        let z_m = if z == 0 {
            Neighbor::OtherChunk(Block(self.0 + BLOCKS_IN_CHUNK - BLOCKS_IN_CHUNK_LAYER))
        } else {
            Neighbor::ThisChunk(Block(self.0 - BLOCKS_IN_CHUNK_LAYER))
        };

        let z_p = if z + 1 < BLOCKS_IN_CHUNK_EDGE {
            Neighbor::ThisChunk(Block(self.0 + BLOCKS_IN_CHUNK_LAYER))
        } else {
            Neighbor::OtherChunk(Block(self.0 + BLOCKS_IN_CHUNK_LAYER - BLOCKS_IN_CHUNK))
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
                block += BLOCKS_IN_CHUNK_EDGE_I32;
            }

            (chunk_offset, block)
        });

        let actual_chunk = Chunk {
            position: Vec3::new(
                chunks_blocks[0].0 + chunk.position[0],
                chunks_blocks[1].0 + chunk.position[1],
                chunks_blocks[2].0 + chunk.position[2],
            ),
            dimension: chunk.dimension,
        };

        let block = Self::from_coords([
            chunks_blocks[0].1 as usize,
            chunks_blocks[1].1 as usize,
            chunks_blocks[2].1 as usize,
        ]);

        (actual_chunk, block)
    }
}

pub enum Neighbor {
    ThisChunk(Block),
    OtherChunk(Block),
}
