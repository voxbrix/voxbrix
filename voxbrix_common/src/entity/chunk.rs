use serde::{
    Deserialize,
    Serialize,
};
use std::{
    cmp::Ordering,
    mem,
};

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Dimension {
    pub index: u32,
}

impl Dimension {
    pub fn to_be_bytes(self) -> [u8; mem::size_of::<Self>()] {
        self.index.to_be_bytes()
    }

    pub fn from_be_bytes(bytes: [u8; mem::size_of::<Self>()]) -> Self {
        Self {
            index: u32::from_be_bytes(bytes),
        }
    }
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Copy, Clone, Debug)]
pub struct Chunk {
    pub position: [i32; 3],
    pub dimension: Dimension,
}

impl Ord for Chunk {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.dimension.cmp(&other.dimension) {
            Ordering::Equal => {
                match self.position[2].cmp(&other.position[2]) {
                    Ordering::Equal => {
                        match self.position[1].cmp(&other.position[1]) {
                            Ordering::Equal => self.position[0].cmp(&other.position[0]),
                            o => o,
                        }
                    },
                    o => o,
                }
            },
            o => o,
        }
    }
}

impl PartialOrd for Chunk {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Chunk {
    pub fn radius(&self, radius: i32) -> ChunkRadius {
        ChunkRadius {
            dimension: self.dimension,
            min_position: self.position.map(|i| i.saturating_sub(radius)),
            max_position: self.position.map(|i| i.saturating_add(radius)),
        }
    }

    pub fn checked_add(&self, offset: [i32; 3]) -> Option<Self> {
        Some(Self {
            position: [
                self.position[0].checked_add(offset[0])?,
                self.position[1].checked_add(offset[1])?,
                self.position[2].checked_add(offset[2])?,
            ],
            dimension: self.dimension,
        })
    }

    pub fn checked_sub(&self, offset: [i32; 3]) -> Option<Self> {
        Some(Self {
            position: [
                self.position[0].checked_sub(offset[0])?,
                self.position[1].checked_sub(offset[1])?,
                self.position[2].checked_sub(offset[2])?,
            ],
            dimension: self.dimension,
        })
    }

    pub fn saturating_add(&self, offset: [i32; 3]) -> Self {
        Self {
            position: [
                self.position[0].saturating_add(offset[0]),
                self.position[1].saturating_add(offset[1]),
                self.position[2].saturating_add(offset[2]),
            ],
            dimension: self.dimension,
        }
    }

    pub fn saturating_sub(&self, offset: [i32; 3]) -> Self {
        Self {
            position: [
                self.position[0].saturating_sub(offset[0]),
                self.position[1].saturating_sub(offset[1]),
                self.position[2].saturating_sub(offset[2]),
            ],
            dimension: self.dimension,
        }
    }
}

pub trait ChunkPositionOperations
where
    Self: Sized,
{
    fn checked_add(&self, offset: Self) -> Option<Self>;

    fn checked_sub(&self, offset: Self) -> Option<Self>;

    fn saturating_add(&self, offset: Self) -> Self;

    fn saturating_sub(&self, offset: Self) -> Self;
}

impl ChunkPositionOperations for [i32; 3] {
    fn checked_add(&self, offset: Self) -> Option<Self> {
        Some([
            self[0].checked_add(offset[0])?,
            self[1].checked_add(offset[1])?,
            self[2].checked_add(offset[2])?,
        ])
    }

    fn checked_sub(&self, offset: Self) -> Option<Self> {
        Some([
            self[0].checked_sub(offset[0])?,
            self[1].checked_sub(offset[1])?,
            self[2].checked_sub(offset[2])?,
        ])
    }

    fn saturating_add(&self, offset: Self) -> Self {
        [
            self[0].saturating_add(offset[0]),
            self[1].saturating_add(offset[1]),
            self[2].saturating_add(offset[2]),
        ]
    }

    fn saturating_sub(&self, offset: Self) -> Self {
        [
            self[0].saturating_sub(offset[0]),
            self[1].saturating_sub(offset[1]),
            self[2].saturating_sub(offset[2]),
        ]
    }
}

#[derive(Clone, Copy)]
pub struct ChunkRadius {
    dimension: Dimension,
    min_position: [i32; 3],
    max_position: [i32; 3],
}

impl ChunkRadius {
    pub fn is_within(&self, chunk: &Chunk) -> bool {
        chunk.dimension == self.dimension
            && chunk.position[0] >= self.min_position[0]
            && chunk.position[0] < self.max_position[0]
            && chunk.position[1] >= self.min_position[1]
            && chunk.position[1] < self.max_position[1]
            && chunk.position[2] >= self.min_position[2]
            && chunk.position[2] < self.max_position[2]
    }

    // TODO: Proper IntoIterator impl
    // https://github.com/rust-lang/rust/issues/63063
    pub fn into_iter(self) -> impl Iterator<Item = Chunk> {
        (self.min_position[2] .. self.max_position[2]).flat_map(move |z| {
            (self.min_position[1] .. self.max_position[1]).flat_map(move |y| {
                (self.min_position[0] .. self.max_position[0]).map(move |x| {
                    Chunk {
                        position: [x, y, z],
                        dimension: self.dimension,
                    }
                })
            })
        })
    }
}
