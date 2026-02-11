pub mod actor;
pub mod actor_class;
pub mod block;
pub mod block_class;
pub mod block_environment;
pub mod chunk;
pub mod dimension_kind;

use crate::{
    resource::component_map::{
        ComponentMap,
        ComponentMapEntity,
    },
    AsFromUsize,
    FromDescriptor,
    LabelLibrary,
};
use anyhow::{
    Context,
    Error,
};
use std::marker::PhantomData;
use voxbrix_world::{
    Initialization,
    World,
};

pub struct StaticEntityComponent<E, T> {
    data: Vec<T>,
    _entity: PhantomData<E>,
}

impl<E, T> StaticEntityComponent<E, T>
where
    E: AsFromUsize,
{
    pub fn get(&self, entity: &E) -> &T {
        &self.data[entity.as_usize()]
    }
}

impl<E, T> Initialization for StaticEntityComponent<E, T>
where
    E: ComponentMapEntity,
    T: FromDescriptor + Default + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let label_library = world.get_resource_ref::<LabelLibrary>();

        let label_map = label_library.get_label_map_for::<E>().ok_or_else(|| {
            anyhow::anyhow!("{} label map is undefined", std::any::type_name::<E>(),)
        })?;

        let component_map = world.get_resource_ref::<ComponentMap<E>>();

        let mut data = Vec::new();
        data.resize_with(label_map.len(), Default::default);

        for res in component_map.get_component::<T::Descriptor>(T::COMPONENT_NAME) {
            let (e, d) = res?;

            data[e.as_usize()] = T::from_descriptor(d, world).with_context(|| {
                let label = label_map.get_label(&e).unwrap_or("UNKNOWN");
                format!(
                    "parsing \"{}\" component for entity \"{}\"",
                    T::COMPONENT_NAME,
                    label
                )
            })?;
        }

        Ok(Self {
            data,
            _entity: PhantomData,
        })
    }
}
