use crate::{
    assets::{
        BLOCK_CLASS_DIR,
        BLOCK_CLASS_LIST_PATH,
    },
    resource::component_map::ComponentMapEntity,
    AsFromUsize,
    StaticEntity,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Debug)]
pub struct BlockClass(pub u16);

impl AsFromUsize for BlockClass {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl ComponentMapEntity for BlockClass {
    const COMPONENT_MAP_DIR: &str = BLOCK_CLASS_DIR;
}

impl StaticEntity for BlockClass {
    const LIST_PATH: &str = BLOCK_CLASS_LIST_PATH;
}
