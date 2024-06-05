use crate::AsFromUsize;
use bincode::{
    Decode,
    Encode,
};

#[derive(Encode, Decode, PartialEq, Eq, Copy, Clone, Debug)]
pub struct ActorClass(pub u64);

impl AsFromUsize for ActorClass {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
