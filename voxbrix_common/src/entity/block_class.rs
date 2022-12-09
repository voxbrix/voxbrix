use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
pub struct BlockClass(pub usize);
