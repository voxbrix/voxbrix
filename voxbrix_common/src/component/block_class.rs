use crate::{
    entity::block_class::BlockClass,
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

pub mod collision;
pub mod opacity;

pub struct BlockClassComponent<T> {
    classes: Vec<T>,
}

impl<T> BlockClassComponent<T> {
    pub fn get(&self, block_class: &BlockClass) -> &T {
        &self.classes[block_class.as_usize()]
    }
}

impl<T> Initialization for BlockClassComponent<T>
where
    T: FromDescriptor + Default + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let mut vec = Vec::new();

        let label_map = world
            .get_resource_ref::<LabelLibrary>()
            .get_label_map_for::<BlockClass>()
            .ok_or_else(|| anyhow::anyhow!("BlockClass label map is undefined"))?
            .clone();
        let component_map = world.get_resource_ref::<ComponentMap<BlockClass>>();

        vec.resize_with(label_map.len(), Default::default);

        for res in component_map.get_component::<T::Descriptor>(T::COMPONENT_NAME) {
            let (e, d) = res?;

            vec[e.as_usize()] = T::from_descriptor(d, world).with_context(|| {
                let label = label_map.get_label(&e).unwrap_or("UNKNOWN");
                format!(
                    "parsing \"{}\" component for entity \"{}\"",
                    T::COMPONENT_NAME,
                    label
                )
            })?;
        }

        Ok(Self { classes: vec })
    }
}
