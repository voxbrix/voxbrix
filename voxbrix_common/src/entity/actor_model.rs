use crate::AsFromUsize;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorModel(pub u64);

impl AsFromUsize for ActorModel {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}
