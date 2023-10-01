use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Debug)]
pub struct ActorClass(pub u64);

impl ActorClass {
    pub fn from_usize(value: usize) -> Self {
        Self(value.try_into().expect("value is out of bounds"))
    }

    pub fn into_usize(self) -> usize {
        self.0.try_into().expect("value is out of bounds")
    }
}
