use crate::math::Vec3;
use serde::{
    Deserialize,
    Serialize,
};
use std::cmp::Ordering;

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Copy, Clone, Debug)]
pub struct Chunk {
    pub position: Vec3<i32>,
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
                            o => return o,
                        }
                    },
                    o => return o,
                }
            },
            o => return o,
        }
    }
}

impl PartialOrd for Chunk {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
