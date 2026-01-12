use crate::{
    assets::{
        BLOCK_ENVIRONMENT_DIR,
        BLOCK_ENVIRONMENT_LIST_PATH,
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
pub struct BlockEnvironment(pub u8);

impl AsFromUsize for BlockEnvironment {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl ComponentMapEntity for BlockEnvironment {
    const COMPONENT_MAP_DIR: &str = BLOCK_ENVIRONMENT_DIR;
}

impl StaticEntity for BlockEnvironment {
    const LIST_PATH: &str = BLOCK_ENVIRONMENT_LIST_PATH;
}
