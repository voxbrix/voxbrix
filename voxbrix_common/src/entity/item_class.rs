use crate::{
    assets::{
        ITEM_CLASS_DIR,
        ITEM_CLASS_LIST_PATH,
    },
    resource::component_map::ComponentMapEntity,
    AsFromUsize,
    StaticEntity,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug)]
pub struct ItemClass(pub u32);

impl AsFromUsize for ItemClass {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl ComponentMapEntity for ItemClass {
    const COMPONENT_MAP_DIR: &str = ITEM_CLASS_DIR;
}

impl StaticEntity for ItemClass {
    const LIST_PATH: &str = ITEM_CLASS_LIST_PATH;
}
