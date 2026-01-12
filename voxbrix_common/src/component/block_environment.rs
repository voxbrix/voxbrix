use crate::{
    entity::block_environment::BlockEnvironment,
    resource::component_map::ComponentMap,
    AsFromUsize,
    FromDescriptor,
    LabelLibrary,
};
use anyhow::{
    Context,
    Error,
};
use voxbrix_world::{
    Initialization,
    World,
};

pub struct BlockEnvironmentComponent<T> {
    environments: Vec<T>,
}

impl<T> BlockEnvironmentComponent<T> {
    pub fn get(&self, block_environment: &BlockEnvironment) -> &T {
        &self.environments[block_environment.as_usize()]
    }
}

impl<T> Initialization for BlockEnvironmentComponent<T>
where
    T: FromDescriptor + Default + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let mut environments = Vec::new();

        let label_map = world
            .get_resource_ref::<LabelLibrary>()
            .get_label_map_for::<BlockEnvironment>()
            .ok_or_else(|| anyhow::anyhow!("BlockEnvironment label map is undefined"))?
            .clone();
        let component_map = world.get_resource_ref::<ComponentMap<BlockEnvironment>>();

        environments.resize_with(label_map.len(), Default::default);

        for res in component_map.get_component::<T::Descriptor>(T::COMPONENT_NAME) {
            let (e, d) = res?;

            environments[e.as_usize()] = T::from_descriptor(d, world).with_context(|| {
                let label = label_map.get_label(&e).unwrap_or("UNKNOWN");
                format!(
                    "parsing \"{}\" component for entity \"{}\"",
                    T::COMPONENT_NAME,
                    label
                )
            })?;
        }

        Ok(Self { environments })
    }
}
