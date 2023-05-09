use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
pub struct ActorClass(pub usize);
