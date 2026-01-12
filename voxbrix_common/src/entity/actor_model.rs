use crate::{
    assets::{
        ACTOR_MODEL_DIR,
        ACTOR_MODEL_LIST_PATH,
    },
    resource::component_map::ComponentMapEntity,
    AsFromUsize,
    FromDescriptor,
    LabelLibrary,
    StaticEntity,
};
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ActorModel(pub u32);

impl AsFromUsize for ActorModel {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl ComponentMapEntity for ActorModel {
    const COMPONENT_MAP_DIR: &str = ACTOR_MODEL_DIR;
}

impl StaticEntity for ActorModel {
    const LIST_PATH: &str = ACTOR_MODEL_LIST_PATH;
}

impl FromDescriptor for ActorModel {
    type Descriptor = String;

    const COMPONENT_NAME: &str = "model";

    fn from_descriptor(desc: Option<Self::Descriptor>, world: &World) -> Result<Self, Error> {
        let label = desc.ok_or_else(|| Error::msg("model descriptor is missing"))?;

        world
            .get_resource_ref::<LabelLibrary>()
            .get(&label)
            .ok_or_else(|| anyhow::anyhow!("actor model \"{}\" is undefined", &label))
    }
}
