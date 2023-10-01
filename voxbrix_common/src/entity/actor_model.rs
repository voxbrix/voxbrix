use serde::{
    Deserialize,
    Serialize,
};

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Serialize, Deserialize, Debug)]
pub struct ActorModel(pub u64);

impl ActorModel {
    pub fn from_usize(value: usize) -> Self {
        Self(value.try_into().expect("value is out of bounds"))
    }

    pub fn into_usize(self) -> usize {
        self.0.try_into().expect("value is out of bounds")
    }
}
