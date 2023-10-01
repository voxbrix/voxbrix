use crate::math::Vec3I32;
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
    pub position: Vec3I32,
    pub dimension: Dimension,
}

impl Ord for Chunk {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.dimension.cmp(&other.dimension) {
            Ordering::Equal => {
                match self.position.z.cmp(&other.position.z) {
                    Ordering::Equal => {
                        match self.position.y.cmp(&other.position.y) {
                            Ordering::Equal => self.position.x.cmp(&other.position.x),
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
            min_position: (
                self.position.x.saturating_sub(radius),
                self.position.y.saturating_sub(radius),
                self.position.z.saturating_sub(radius),
            ),
            max_position: (
                self.position.x.saturating_add(radius),
                self.position.y.saturating_add(radius),
                self.position.z.saturating_add(radius),
            ),
        }
    }

    pub fn offset(&self, offset: Vec3I32) -> Option<Self> {
        Some(Self {
            position: Vec3I32::new(
                self.position.x.checked_add(offset[0])?,
                self.position.y.checked_add(offset[1])?,
                self.position.z.checked_add(offset[2])?,
            ),
            dimension: self.dimension,
        })
    }
}

#[derive(Clone, Copy)]
pub struct ChunkRadius {
    dimension: Dimension,
    min_position: (i32, i32, i32),
    max_position: (i32, i32, i32),
}

impl ChunkRadius {
    pub fn is_within(&self, chunk: &Chunk) -> bool {
        chunk.dimension == self.dimension
            && chunk.position.x >= self.min_position.0
            && chunk.position.x < self.max_position.0
            && chunk.position.y >= self.min_position.1
            && chunk.position.y < self.max_position.1
            && chunk.position.z >= self.min_position.2
            && chunk.position.z < self.max_position.2
    }

    // TODO: Proper IntoIterator impl
    // https://github.com/rust-lang/rust/issues/63063
    pub fn into_iter(self) -> impl Iterator<Item = Chunk> {
        (self.min_position.2 .. self.max_position.2).flat_map(move |z| {
            (self.min_position.1 .. self.max_position.1).flat_map(move |y| {
                (self.min_position.0 .. self.max_position.0).map(move |x| {
                    Chunk {
                        position: Vec3I32 { x, y, z },
                        dimension: self.dimension,
                    }
                })
            })
        })
    }
}
