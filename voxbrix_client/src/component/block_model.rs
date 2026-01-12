use crate::entity::block_model::BlockModel;
use anyhow::{
    Context,
    Error,
};
use voxbrix_common::{
    resource::component_map::ComponentMap,
    AsFromUsize,
    FromDescriptor,
    LabelLibrary,
};
use voxbrix_world::{
    Initialization,
    World,
};

pub mod builder;
pub mod culling;

pub struct BlockModelComponent<T> {
    models: Vec<T>,
}

impl<T> BlockModelComponent<T> {
    pub fn get(&self, block_model: &BlockModel) -> &T {
        &self.models[block_model.as_usize()]
    }
}

impl<T> Initialization for BlockModelComponent<T>
where
    T: FromDescriptor + Default + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let mut vec = Vec::new();

        let label_map = world
            .get_resource_ref::<LabelLibrary>()
            .get_label_map_for::<BlockModel>()
            .ok_or_else(|| anyhow::anyhow!("BlockModel label map is undefined"))?
            .clone();
        let component_map = world.get_resource_ref::<ComponentMap<BlockModel>>();

        vec.resize_with(label_map.len(), Default::default);

        for res in component_map.get_component::<T::Descriptor>(T::COMPONENT_NAME) {
            let (e, d) = res?;

            vec[e.as_usize()] = T::from_descriptor(d, world).with_context(|| {
                let label = label_map.get_label(&e).unwrap_or("UNKNOWN");
                format!(
                    "parsing {} component for entity {}",
                    T::COMPONENT_NAME,
                    label
                )
            })?;
        }

        Ok(Self { models: vec })
    }
}
