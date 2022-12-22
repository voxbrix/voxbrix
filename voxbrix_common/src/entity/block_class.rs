use crate::pack::PackDefault;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Debug)]
pub struct BlockClass(pub usize);

impl PackDefault for BlockClass {}
