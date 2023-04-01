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

    /// Must provide correct block coords,
    /// you have to make `Block` from coords with `.from_coords()` or
    /// extract coords from the `Block` with `.to_coords()`
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

    /// Must provide correct block coords,
    /// you have to make `Block` from coords with `.from_coords()` or
    /// extract coords from the `Block` with `.to_coords()`
    pub fn same_chunk_neighbors(&self, [x, y, z]: [usize; 3]) -> [Option<(Block, [usize; 3])>; 6] {
        let x_m = if x == 0 {
            None
        } else {
            Some((Block(self.0 - 1), [x - 1, y, z]))
        };

        let x_p = if x + 1 < BLOCKS_IN_CHUNK_EDGE {
            Some((Block(self.0 + 1), [x + 1, y, z]))
        } else {
            None
        };

        let y_m = if y == 0 {
            None
        } else {
            Some((Block(self.0 - BLOCKS_IN_CHUNK_EDGE), [x, y - 1, z]))
        };

        let y_p = if y + 1 < BLOCKS_IN_CHUNK_EDGE {
            Some((Block(self.0 + BLOCKS_IN_CHUNK_EDGE), [x, y + 1, z]))
        } else {
            None
        };

        let z_m = if z == 0 {
            None
        } else {
            Some((Block(self.0 - BLOCKS_IN_CHUNK_LAYER), [x, y, z - 1]))
        };

        let z_p = if z + 1 < BLOCKS_IN_CHUNK_EDGE {
            Some((Block(self.0 + BLOCKS_IN_CHUNK_LAYER), [x, y, z + 1]))
        } else {
            None
        };

        [x_m, x_p, y_m, y_p, z_m, z_p]
    }

    /// Must provide correct block coords,
    /// you have to make `Block` from coords with `.from_coords()` or
    /// extract coords from the `Block` with `.to_coords()`
    pub fn neighbor_with_coords_side(
        &self,
        side: usize,
        [x, y, z]: [usize; 3],
    ) -> NeighborWithCoords {
        match side {
            0 => {
                if x == 0 {
                    NeighborWithCoords::OtherChunk(
                        Block(self.0 + BLOCKS_IN_CHUNK_EDGE - 1),
                        [BLOCKS_IN_CHUNK_EDGE - 1, y, z],
                    )
                } else {
                    NeighborWithCoords::ThisChunk(Block(self.0 - 1), [x - 1, y, z])
                }
            },
            1 => {
                if x + 1 < BLOCKS_IN_CHUNK_EDGE {
                    NeighborWithCoords::ThisChunk(Block(self.0 + 1), [x + 1, y, z])
                } else {
                    NeighborWithCoords::OtherChunk(
                        Block(self.0 + 1 - BLOCKS_IN_CHUNK_EDGE),
                        [0, y, z],
                    )
                }
            },
            2 => {
                if y == 0 {
                    NeighborWithCoords::OtherChunk(
                        Block(self.0 + BLOCKS_IN_CHUNK_LAYER - BLOCKS_IN_CHUNK_EDGE),
                        [x, BLOCKS_IN_CHUNK_EDGE - 1, z],
                    )
                } else {
                    NeighborWithCoords::ThisChunk(
                        Block(self.0 - BLOCKS_IN_CHUNK_EDGE),
                        [x, y - 1, z],
                    )
                }
            },
            3 => {
                if y + 1 < BLOCKS_IN_CHUNK_EDGE {
                    NeighborWithCoords::ThisChunk(
                        Block(self.0 + BLOCKS_IN_CHUNK_EDGE),
                        [x, y + 1, z],
                    )
                } else {
                    NeighborWithCoords::OtherChunk(
                        Block(self.0 + BLOCKS_IN_CHUNK_EDGE - BLOCKS_IN_CHUNK_LAYER),
                        [x, 0, z],
                    )
                }
            },
            4 => {
                if z == 0 {
                    NeighborWithCoords::OtherChunk(
                        Block(self.0 + BLOCKS_IN_CHUNK - BLOCKS_IN_CHUNK_LAYER),
                        [x, y, BLOCKS_IN_CHUNK_EDGE - 1],
                    )
                } else {
                    NeighborWithCoords::ThisChunk(
                        Block(self.0 - BLOCKS_IN_CHUNK_LAYER),
                        [x, y, z - 1],
                    )
                }
            },
            5 => {
                if z + 1 < BLOCKS_IN_CHUNK_EDGE {
                    NeighborWithCoords::ThisChunk(
                        Block(self.0 + BLOCKS_IN_CHUNK_LAYER),
                        [x, y, z + 1],
                    )
                } else {
                    NeighborWithCoords::OtherChunk(
                        Block(self.0 + BLOCKS_IN_CHUNK_LAYER - BLOCKS_IN_CHUNK),
                        [x, y, 0],
                    )
                }
            },
            i => panic!("incorrect side index: {}", i),
        }
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

pub enum NeighborWithCoords {
    ThisChunk(Block, [usize; 3]),
    OtherChunk(Block, [usize; 3]),
}
