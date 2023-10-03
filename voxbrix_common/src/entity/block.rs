use crate::{
    entity::chunk::Chunk,
    math::Vec3I32,
};
use serde::{
    Deserialize,
    Serialize,
};

pub const BLOCKS_IN_CHUNK_EDGE: u16 = 32;
pub const BLOCKS_IN_CHUNK_LAYER: u16 = BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;
pub const BLOCKS_IN_CHUNK: u16 = BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;

pub const BLOCKS_IN_CHUNK_EDGE_USIZE: usize = BLOCKS_IN_CHUNK_EDGE as usize;
pub const BLOCKS_IN_CHUNK_LAYER_USIZE: usize = BLOCKS_IN_CHUNK_LAYER as usize;
pub const BLOCKS_IN_CHUNK_USIZE: usize = BLOCKS_IN_CHUNK as usize;

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Block(pub u16);

pub type BlockCoords = [u16; 3];

impl std::hash::Hash for Block {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u16(self.0)
    }
}

impl nohash_hasher::IsEnabled for Block {}

impl Block {
    pub fn from_usize(value: usize) -> Self {
        Self(value.try_into().expect("value is out of bounds"))
    }

    pub fn into_usize(self) -> usize {
        self.0.try_into().expect("value is out of bounds")
    }
}

impl Block {
    pub fn to_coords(&self) -> BlockCoords {
        let z = self.0 / BLOCKS_IN_CHUNK_LAYER;
        let x_y = self.0 % BLOCKS_IN_CHUNK_LAYER;
        let y = x_y / BLOCKS_IN_CHUNK_EDGE;
        let x = x_y % BLOCKS_IN_CHUNK_EDGE;

        [x, y, z]
    }

    pub fn from_coords([x, y, z]: BlockCoords) -> Self {
        Self(z * BLOCKS_IN_CHUNK_LAYER + y * BLOCKS_IN_CHUNK_EDGE + x)
    }

    /// Must provide correct block coords,
    /// you have to make `Block` from coords with `.from_coords()` or
    /// extract coords from the `Block` with `.to_coords()`
    pub fn neighbors_in_coords(&self, [x, y, z]: BlockCoords) -> [Neighbor; 6] {
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
    pub fn same_chunk_neighbors(
        &self,
        [x, y, z]: BlockCoords,
    ) -> [Option<(Block, BlockCoords)>; 6] {
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
        side: u16,
        [x, y, z]: BlockCoords,
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

    // TODO: check if the chunk is on the edge of the map
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
            position: Vec3I32::new(
                chunks_blocks[0].0 + chunk.position[0],
                chunks_blocks[1].0 + chunk.position[1],
                chunks_blocks[2].0 + chunk.position[2],
            ),
            dimension: chunk.dimension,
        };

        let block = Self::from_coords([
            chunks_blocks[0].1 as u16,
            chunks_blocks[1].1 as u16,
            chunks_blocks[2].1 as u16,
        ]);

        (actual_chunk, block)
    }
}

pub enum Neighbor {
    ThisChunk(Block),
    OtherChunk(Block),
}

pub enum NeighborWithCoords {
    ThisChunk(Block, BlockCoords),
    OtherChunk(Block, BlockCoords),
}
