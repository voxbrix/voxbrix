use bincode::{
    Decode,
    Encode,
};

#[derive(Encode, Decode, PartialEq, Eq, Copy, Clone, Debug)]
pub struct BlockClass(pub u64);

impl BlockClass {
    pub fn from_usize(value: usize) -> Self {
        Self(value.try_into().expect("value is out of bounds"))
    }

    pub fn into_usize(self) -> usize {
        self.0.try_into().expect("value is out of bounds")
    }
}
