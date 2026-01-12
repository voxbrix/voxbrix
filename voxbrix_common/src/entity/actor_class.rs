use crate::{
    assets::{
        ACTOR_CLASS_DIR,
        ACTOR_CLASS_LIST_PATH,
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
pub struct ActorClass(pub u64);

impl AsFromUsize for ActorClass {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl ComponentMapEntity for ActorClass {
    const COMPONENT_MAP_DIR: &str = ACTOR_CLASS_DIR;
}

impl StaticEntity for ActorClass {
    const LIST_PATH: &str = ACTOR_CLASS_LIST_PATH;
}
