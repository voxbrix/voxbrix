use crate::assets::{
    BLOCK_MODEL_DIR,
    BLOCK_MODEL_LIST_PATH,
};
use anyhow::Error;
use voxbrix_common::{
    resource::component_map::ComponentMapEntity,
    AsFromUsize,
    FromDescriptor,
    LabelLibrary,
    StaticEntity,
};
use voxbrix_world::World;

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct BlockModel(pub u32);

impl AsFromUsize for BlockModel {
    fn as_usize(&self) -> usize {
        self.0.try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        Self(i.try_into().unwrap())
    }
}

impl std::hash::Hash for BlockModel {
    fn hash<H: std::hash::Hasher>(&self, hasher: &mut H) {
        hasher.write_u32(self.0)
    }
}

impl nohash_hasher::IsEnabled for BlockModel {}

impl StaticEntity for BlockModel {
    const LIST_PATH: &str = BLOCK_MODEL_LIST_PATH;
}

impl ComponentMapEntity for BlockModel {
    const COMPONENT_MAP_DIR: &str = BLOCK_MODEL_DIR;
}

impl FromDescriptor for BlockModel {
    type Descriptor = String;

    const COMPONENT_NAME: &str = "model";

    fn from_descriptor(desc: Option<Self::Descriptor>, world: &World) -> Result<Self, Error> {
        let label = desc.ok_or_else(|| Error::msg("model descriptor is missing"))?;

        world
            .get_resource_ref::<LabelLibrary>()
            .get(&label)
            .ok_or_else(|| anyhow::anyhow!("block model \"{}\" is undefined", &label))
    }
}
