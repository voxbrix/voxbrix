use std::cmp::Ordering;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct Chunk {
    pub position: [i32; 3],
    pub dimention: u32,
}

impl Ord for Chunk {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.dimention.cmp(&other.dimention) {
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
