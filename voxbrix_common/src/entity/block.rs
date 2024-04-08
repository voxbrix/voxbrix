use crate::entity::chunk::Chunk;
use bincode::{
    de::Decoder,
    enc::Encoder,
    error::{
        DecodeError,
        EncodeError,
    },
    Decode,
    Encode,
};

pub const BLOCKS_IN_CHUNK_EDGE: usize = 16;
pub const BLOCKS_IN_CHUNK_LAYER: usize = BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;
pub const BLOCKS_IN_CHUNK: usize =
    BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE * BLOCKS_IN_CHUNK_EDGE;

pub const BLOCKS_IN_CHUNK_EDGE_F32: f32 = BLOCKS_IN_CHUNK_EDGE as f32;

pub const BLOCKS_IN_CHUNK_EDGE_I32: i32 = BLOCKS_IN_CHUNK_EDGE as i32;

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Block(pub usize);

pub type BlockCoords = [usize; 3];

impl std::hash::Hash for Block {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u16(self.0.try_into().unwrap())
    }
}

impl nohash_hasher::IsEnabled for Block {}

impl Decode for Block {
    fn decode<D>(decoder: &mut D) -> Result<Self, DecodeError>
    where
        D: Decoder,
    {
        let block: usize = u16::decode(decoder)?.try_into().unwrap();

        if block > BLOCKS_IN_CHUNK {
            return Err(DecodeError::LimitExceeded);
        }

        Ok(Block(block))
    }
}

bincode::impl_borrow_decode!(Block);

impl Encode for Block {
    fn encode<E>(&self, encoder: &mut E) -> Result<(), EncodeError>
    where
        E: Encoder,
    {
        let value: u16 = self.0.try_into().unwrap();
        value.encode(encoder)
    }
}

impl Block {
    pub fn into_coords(self) -> BlockCoords {
        let z = self.0 / BLOCKS_IN_CHUNK_LAYER;
        let x_y = self.0 % BLOCKS_IN_CHUNK_LAYER;
        let y = x_y / BLOCKS_IN_CHUNK_EDGE;
        let x = x_y % BLOCKS_IN_CHUNK_EDGE;

        [x, y, z]
    }

    pub fn from_coords([x, y, z]: BlockCoords) -> Self {
        Self(z * BLOCKS_IN_CHUNK_LAYER + y * BLOCKS_IN_CHUNK_EDGE + x)
    }

    pub fn neighbors(&self) -> [Neighbor; 6] {
        let [x, y, z] = self.into_coords();

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
    pub fn same_chunk_neighbors(&self) -> [Option<Block>; 6] {
        let [x, y, z] = self.into_coords();
        let x_m = if x == 0 {
            None
        } else {
            Some(Block(self.0 - 1))
        };

        let x_p = if x + 1 < BLOCKS_IN_CHUNK_EDGE {
            Some(Block(self.0 + 1))
        } else {
            None
        };

        let y_m = if y == 0 {
            None
        } else {
            Some(Block(self.0 - BLOCKS_IN_CHUNK_EDGE))
        };

        let y_p = if y + 1 < BLOCKS_IN_CHUNK_EDGE {
            Some(Block(self.0 + BLOCKS_IN_CHUNK_EDGE))
        } else {
            None
        };

        let z_m = if z == 0 {
            None
        } else {
            Some(Block(self.0 - BLOCKS_IN_CHUNK_LAYER))
        };

        let z_p = if z + 1 < BLOCKS_IN_CHUNK_EDGE {
            Some(Block(self.0 + BLOCKS_IN_CHUNK_LAYER))
        } else {
            None
        };

        [x_m, x_p, y_m, y_p, z_m, z_p]
    }

    pub fn neighbor_on_side(&self, side: u16) -> NeighborWithCoords {
        let [x, y, z] = self.into_coords();

        match side {
            0 => {
                if x == 0 {
                    NeighborWithCoords::OtherChunk(Block(self.0 + BLOCKS_IN_CHUNK_EDGE - 1))
                } else {
                    NeighborWithCoords::ThisChunk(Block(self.0 - 1))
                }
            },
            1 => {
                if x + 1 < BLOCKS_IN_CHUNK_EDGE {
                    NeighborWithCoords::ThisChunk(Block(self.0 + 1))
                } else {
                    NeighborWithCoords::OtherChunk(Block(self.0 + 1 - BLOCKS_IN_CHUNK_EDGE))
                }
            },
            2 => {
                if y == 0 {
                    NeighborWithCoords::OtherChunk(Block(
                        self.0 + BLOCKS_IN_CHUNK_LAYER - BLOCKS_IN_CHUNK_EDGE,
                    ))
                } else {
                    NeighborWithCoords::ThisChunk(Block(self.0 - BLOCKS_IN_CHUNK_EDGE))
                }
            },
            3 => {
                if y + 1 < BLOCKS_IN_CHUNK_EDGE {
                    NeighborWithCoords::ThisChunk(Block(self.0 + BLOCKS_IN_CHUNK_EDGE))
                } else {
                    NeighborWithCoords::OtherChunk(Block(
                        self.0 + BLOCKS_IN_CHUNK_EDGE - BLOCKS_IN_CHUNK_LAYER,
                    ))
                }
            },
            4 => {
                if z == 0 {
                    NeighborWithCoords::OtherChunk(Block(
                        self.0 + BLOCKS_IN_CHUNK - BLOCKS_IN_CHUNK_LAYER,
                    ))
                } else {
                    NeighborWithCoords::ThisChunk(Block(self.0 - BLOCKS_IN_CHUNK_LAYER))
                }
            },
            5 => {
                if z + 1 < BLOCKS_IN_CHUNK_EDGE {
                    NeighborWithCoords::ThisChunk(Block(self.0 + BLOCKS_IN_CHUNK_LAYER))
                } else {
                    NeighborWithCoords::OtherChunk(Block(
                        self.0 + BLOCKS_IN_CHUNK_LAYER - BLOCKS_IN_CHUNK,
                    ))
                }
            },
            i => panic!("incorrect side index: {}", i),
        }
    }

    pub fn from_chunk_offset(chunk: Chunk, offset: [i32; 3]) -> Option<(Chunk, Block)> {
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
            position: [
                chunks_blocks[0].0.checked_add(chunk.position[0])?,
                chunks_blocks[1].0.checked_add(chunk.position[1])?,
                chunks_blocks[2].0.checked_add(chunk.position[2])?,
            ],
            dimension: chunk.dimension,
        };

        let block = Self::from_coords([
            chunks_blocks[0].1 as usize,
            chunks_blocks[1].1 as usize,
            chunks_blocks[2].1 as usize,
        ]);

        Some((actual_chunk, block))
    }
}

pub enum Neighbor {
    ThisChunk(Block),
    OtherChunk(Block),
}

pub enum NeighborWithCoords {
    ThisChunk(Block),
    OtherChunk(Block),
}
