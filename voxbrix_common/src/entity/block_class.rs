use crate::AsFromUsize;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Debug)]
pub struct BlockClass(pub u32);

impl AsFromUsize for BlockClass {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
