use crate::AsFromUsize;
use serde::{
    Deserialize,
    Serialize,
};
use std::cmp::Ordering;

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct DimensionKind(pub u32);

impl AsFromUsize for DimensionKind {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct Dimension {
    pub kind: DimensionKind,
    pub phase: u64,
}

impl Dimension {
    pub fn to_be_bytes(self) -> [u8; 12] {
        let mut output = [0u8; 12];

        output[.. 4].copy_from_slice(&self.kind.0.to_be_bytes());
        output[4 ..].copy_from_slice(&self.phase.to_be_bytes());

        output
    }

    pub fn from_be_bytes(bytes: [u8; 12]) -> Self {
        Self {
            kind: DimensionKind(u32::from_be_bytes(bytes[.. 4].try_into().unwrap())),
            phase: u64::from_be_bytes(bytes[4 ..].try_into().unwrap()),
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
            max_position: self.position.map(|i| i.saturating_add(radius - 1)),
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

#[derive(Clone, Copy, Debug)]
pub struct ChunkRadius {
    dimension: Dimension,
    min_position: [i32; 3],
    max_position: [i32; 3],
}

enum EitherIter<A, B, C> {
    A(A),
    B(B),
    C(C),
}

impl<T, A, B, C> Iterator for EitherIter<A, B, C>
where
    A: Iterator<Item = T>,
    B: Iterator<Item = T>,
    C: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::A(iter) => iter.next(),
            Self::B(iter) => iter.next(),
            Self::C(iter) => iter.next(),
        }
    }
}

impl<T, A, B, C> DoubleEndedIterator for EitherIter<A, B, C>
where
    A: DoubleEndedIterator<Item = T>,
    B: DoubleEndedIterator<Item = T>,
    C: DoubleEndedIterator<Item = T>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::A(iter) => iter.next_back(),
            Self::B(iter) => iter.next_back(),
            Self::C(iter) => iter.next_back(),
        }
    }
}

impl ChunkRadius {
    pub fn is_within(&self, chunk: &Chunk) -> bool {
        chunk.dimension == self.dimension
            && chunk.position[0] >= self.min_position[0]
            && chunk.position[0] <= self.max_position[0]
            && chunk.position[1] >= self.min_position[1]
            && chunk.position[1] <= self.max_position[1]
            && chunk.position[2] >= self.min_position[2]
            && chunk.position[2] <= self.max_position[2]
    }

    pub fn into_iter_expanding(self) -> impl DoubleEndedIterator<Item = Chunk> {
        let min_diameter = self
            .min_position
            .iter()
            .zip(self.max_position.iter())
            .map(|(min, max)| max - min)
            .min()
            .unwrap();

        let max_step = {
            let dv = min_diameter / 2;
            let md = min_diameter % 2;

            dv + md
        };

        (0 .. max_step)
            .map(move |step| {
                let min_z = self.min_position[2].saturating_add(step);
                let max_z = self.max_position[2].saturating_sub(step);
                let min_y = self.min_position[1].saturating_add(step);
                let max_y = self.max_position[1].saturating_sub(step);
                let min_x = self.min_position[0].saturating_add(step);
                let max_x = self.max_position[0].saturating_sub(step);

                (min_z, max_z, min_y, max_y, min_x, max_x)
            })
            .rev()
            .flat_map(move |(min_z, max_z, min_y, max_y, min_x, max_x)| {
                (min_z ..= max_z).flat_map(move |z| {
                    (min_y ..= max_y).flat_map(move |y| {
                        if z == min_z || z == max_z || y == min_y || y == max_y {
                            EitherIter::A(min_x ..= max_x)
                        } else if min_x != max_x {
                            EitherIter::B([min_x, max_x].into_iter())
                        } else {
                            EitherIter::C([min_x].into_iter())
                        }
                        .map(move |x| {
                            Chunk {
                                position: [x, y, z],
                                dimension: self.dimension,
                            }
                        })
                    })
                })
            })
    }

    pub fn into_iter_simple(self) -> impl Iterator<Item = Chunk> {
        (self.min_position[2] ..= self.max_position[2]).flat_map(move |z| {
            (self.min_position[1] ..= self.max_position[1]).flat_map(move |y| {
                (self.min_position[0] ..= self.max_position[0]).map(move |x| {
                    Chunk {
                        position: [x, y, z],
                        dimension: self.dimension,
                    }
                })
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_chunk_radius_expanding_iter() {
        let dimension = Dimension {
            kind: DimensionKind(0),
            phase: 0,
        };

        let position = [0, 0, 0];

        let radius = Chunk {
            dimension,
            position,
        }
        .radius(5);

        let chunks_sorted = radius
            .into_iter_expanding()
            .inspect(|chunk| assert!(radius.is_within(chunk)))
            .collect::<Vec<_>>();

        assert_eq!(chunks_sorted.len(), 1000);

        let max_dist_for_index = |index: usize| {
            chunks_sorted[index]
                .position
                .iter()
                .zip(position.iter())
                .map(|(chunk_pos, position)| chunk_pos.abs_diff(*position))
                .max()
                .unwrap()
        };

        for index in 0 .. 8 {
            let max_dist: u32 = max_dist_for_index(index);
            assert!(max_dist <= 1);
        }

        for index in 0 .. 64 {
            let max_dist: u32 = max_dist_for_index(index);
            assert!(max_dist <= 2);
        }

        for index in 0 .. 999 {
            let max_dist_1: u32 = max_dist_for_index(index);
            // + 1 here is the margin between layers' max coordinate distance, for example
            //
            //
            // | <- chunk with max dist -2 (2)
            // |           | <- center chunk 0
            // |           |     | <- chunk with max dist 1 (1)
            // |___________|_____|_____
            // |     |     |     |     |
            // |     |     |     |     |
            // |_____|_____|_____|_____|
            // |---------radius--------|
            //
            // chunks (1) and (2) are in the same outer layer
            let max_dist_2: u32 = max_dist_for_index(index + 1);

            assert!(max_dist_1 <= max_dist_2 + 1);
        }
    }
}
