use crate::math::Vec3I32;
use serde::{
    Deserialize,
    Serialize,
};
use std::cmp::Ordering;

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Copy, Clone, Debug)]
pub struct Chunk {
    pub position: Vec3I32,
    pub dimension: u32,
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
            min_position: (
                self.position[0].saturating_sub(radius),
                self.position[1].saturating_sub(radius),
                self.position[2].saturating_sub(radius),
            ),
            max_position: (
                self.position[0].saturating_add(radius),
                self.position[1].saturating_add(radius),
                self.position[2].saturating_add(radius),
            ),
        }
    }

    pub fn offset(&self, offset: Vec3I32) -> Option<Self> {
        Some(Self {
            position: Vec3I32::new(
                self.position[0].checked_add(offset[0])?,
                self.position[1].checked_add(offset[1])?,
                self.position[2].checked_add(offset[2])?,
            ),
            dimension: self.dimension,
        })
    }
}

#[derive(Clone, Copy)]
pub struct ChunkRadius {
    dimension: u32,
    min_position: (i32, i32, i32),
    max_position: (i32, i32, i32),
}

impl ChunkRadius {
    pub fn is_within(&self, chunk: &Chunk) -> bool {
        chunk.dimension == self.dimension
            && chunk.position[0] >= self.min_position.0
            && chunk.position[0] < self.max_position.0
            && chunk.position[1] >= self.min_position.1
            && chunk.position[1] < self.max_position.1
            && chunk.position[2] >= self.min_position.2
            && chunk.position[2] < self.max_position.2
    }

    // TODO: Proper IntoIterator impl
    // https://github.com/rust-lang/rust/issues/63063
    pub fn into_iter(self) -> impl Iterator<Item = Chunk> {
        (self.min_position.2 .. self.max_position.2).flat_map(move |z| {
            (self.min_position.1 .. self.max_position.1).flat_map(move |y| {
                (self.min_position.0 .. self.max_position.0).map(move |x| {
                    Chunk {
                        position: [x, y, z].into(),
                        dimension: self.dimension,
                    }
                })
            })
        })
    }
}
