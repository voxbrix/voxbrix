use serde::{
    Deserialize,
    Serialize,
};

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Serialize, Deserialize, Debug)]
pub struct ActorModel(pub usize);
