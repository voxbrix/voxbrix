use crate::{
    math::Vec3I32,
    AsFromUsize,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    cmp::Ordering,
    iter,
};

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct DimensionKind(pub u8);

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
    pub const BYTES_LEN: usize = 9;

    pub fn to_be_bytes(self) -> [u8; Self::BYTES_LEN] {
        let mut output = [0u8; Self::BYTES_LEN];

        output[0] = self.kind.0;
        output[1 ..].copy_from_slice(&self.phase.to_be_bytes());

        output
    }

    pub fn from_be_bytes(bytes: [u8; Self::BYTES_LEN]) -> Self {
        Self {
            kind: DimensionKind(bytes[0]),
            phase: u64::from_be_bytes(bytes[1 ..].try_into().unwrap()),
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

    /// Radius around all chunks in between self and other chunk.
    /// Returns [`None`] if the other chunk is in different dimension.
    pub fn radius_for_range(&self, other: &Chunk, radius: i32) -> Option<ChunkRadius> {
        if self.dimension != other.dimension {
            return None;
        }

        let min_chunk_pos = self.position.min(other.position);
        let max_chunk_pos = self.position.max(other.position);

        Some(ChunkRadius {
            dimension: self.dimension,
            min_position: min_chunk_pos.map(|i| i.saturating_sub(radius)),
            max_position: max_chunk_pos.map(|i| i.saturating_add(radius - 1)),
        })
    }

    pub fn checked_add(&self, offset: Vec3I32) -> Option<Self> {
        Some(Self {
            position: self.position.checked_add(offset)?,
            dimension: self.dimension,
        })
    }

    pub fn checked_sub(&self, offset: Vec3I32) -> Option<Self> {
        Some(Self {
            position: self.position.checked_sub(offset)?,
            dimension: self.dimension,
        })
    }

    pub fn saturating_add(&self, offset: Vec3I32) -> Self {
        Self {
            position: self.position.saturating_add(offset),
            dimension: self.dimension,
        }
    }

    pub fn saturating_sub(&self, offset: Vec3I32) -> Self {
        Self {
            position: offset.saturating_sub(offset),
            dimension: self.dimension,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChunkRadius {
    dimension: Dimension,
    min_position: Vec3I32,
    max_position: Vec3I32,
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
            && chunk.position.x >= self.min_position.x
            && chunk.position.x <= self.max_position.x
            && chunk.position.y >= self.min_position.y
            && chunk.position.y <= self.max_position.y
            && chunk.position.z >= self.min_position.z
            && chunk.position.z <= self.max_position.z
    }

    pub fn into_iter_expanding(self) -> impl DoubleEndedIterator<Item = Chunk> {
        let min_diameter = self
            .max_position
            .checked_sub(self.min_position)
            .and_then(|s| s.min_element().checked_add(1))
            .expect("out of bounds");

        let max_step = {
            let dv = min_diameter / 2;
            let md = min_diameter % 2;

            dv + md
        };

        (0 .. max_step)
            .map(move |s| {
                (
                    self.min_position.map(|i| i + s),
                    self.max_position.map(|i| i - s),
                )
            })
            .rev()
            .flat_map(move |(min, max)| {
                (min.z ..= max.z).flat_map(move |z| {
                    (min.y ..= max.y).flat_map(move |y| {
                        if z == min.z || z == max.z || y == min.y || y == max.y {
                            EitherIter::A(min.x ..= max.x)
                        } else if min.x != max.x {
                            EitherIter::B([min.x, max.x].into_iter())
                        } else {
                            EitherIter::C(iter::once(min.x))
                        }
                        .map(move |x| {
                            Chunk {
                                position: Vec3I32::new(x, y, z),
                                dimension: self.dimension,
                            }
                        })
                    })
                })
            })
    }

    pub fn into_iter_simple(self) -> impl Iterator<Item = Chunk> {
        (self.min_position.z ..= self.max_position.z).flat_map(move |z| {
            (self.min_position.y ..= self.max_position.y).flat_map(move |y| {
                (self.min_position.x ..= self.max_position.x).map(move |x| {
                    Chunk {
                        position: Vec3I32::new(x, y, z),
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
        let position = [0, 0, 0];

        let dimension = Dimension {
            kind: DimensionKind(0),
            phase: 0,
        };

        let radius = Chunk {
            dimension,
            position: Vec3I32::from_array(position),
        }
        .radius(5);

        let chunks_sorted = radius
            .into_iter_expanding()
            .inspect(|chunk| assert!(radius.is_within(chunk)))
            .collect::<Vec<_>>();

        let ctrl = radius.into_iter_simple().collect::<Vec<_>>();

        assert_eq!(chunks_sorted.len(), ctrl.len());

        {
            let mut chunks_sorted = chunks_sorted.clone();
            chunks_sorted.sort();

            assert_eq!(chunks_sorted, ctrl);
        }

        let max_dist_for_index = |index: usize| {
            chunks_sorted[index]
                .position
                .to_array()
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

    #[test]
    fn check_chunk_radius_expanding_iter_bounds() {
        fn test_expanding_iter(position: [i32; 3]) {
            let dimension = Dimension {
                kind: DimensionKind(0),
                phase: 0,
            };

            let radius = Chunk {
                dimension,
                position: Vec3I32::from_array(position),
            }
            .radius(5);

            let chunks_sorted = radius
                .into_iter_expanding()
                .inspect(|chunk| assert!(radius.is_within(chunk)))
                .collect::<Vec<_>>();

            let ctrl = radius.into_iter_simple().collect::<Vec<_>>();

            assert_eq!(chunks_sorted.len(), ctrl.len());

            {
                let mut chunks_sorted = chunks_sorted.clone();
                chunks_sorted.sort();

                assert_eq!(chunks_sorted, ctrl);
            }
        }

        test_expanding_iter([i32::MIN, 0, 0]);
        test_expanding_iter([0, i32::MIN, 0]);
        test_expanding_iter([0, i32::MIN, i32::MIN]);
        test_expanding_iter([i32::MIN, 0, i32::MIN]);
        test_expanding_iter([i32::MIN, i32::MIN, 0]);
        test_expanding_iter([i32::MIN, i32::MIN, i32::MIN]);

        test_expanding_iter([i32::MAX, 0, 0]);
        test_expanding_iter([0, i32::MAX, 0]);
        test_expanding_iter([0, i32::MAX, i32::MAX]);
        test_expanding_iter([i32::MAX, 0, i32::MAX]);
        test_expanding_iter([i32::MAX, i32::MAX, 0]);
        test_expanding_iter([i32::MAX, i32::MAX, i32::MAX]);
    }
}
