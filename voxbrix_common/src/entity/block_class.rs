use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Debug)]
pub struct BlockClass(usize);

impl BlockClass {
    pub const fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub fn index(self) -> usize {
        self.0
    }
}
