use crate::{
    entity::chunk::DimensionKind,
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

pub mod sky_light_config;

pub struct DimensionKindComponent<T> {
    data: Vec<T>,
}

impl<T> DimensionKindComponent<T> {
    pub fn get(&self, dimension_kind: &DimensionKind) -> &T {
        &self.data[dimension_kind.as_usize()]
    }
}

impl<T> Initialization for DimensionKindComponent<T>
where
    T: FromDescriptor + Default + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let mut data = Vec::new();

        let label_map = world
            .get_resource_ref::<LabelLibrary>()
            .get_label_map_for::<DimensionKind>()
            .ok_or_else(|| anyhow::anyhow!("DimensionKind label map is undefined"))?
            .clone();
        let component_map = world.get_resource_ref::<ComponentMap<DimensionKind>>();

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

        Ok(Self { data })
    }
}
