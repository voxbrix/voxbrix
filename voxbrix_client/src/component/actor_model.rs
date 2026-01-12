use anyhow::{
    Context,
    Error,
};
use voxbrix_common::{
    entity::actor_model::ActorModel,
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

pub struct ActorModelComponent<T> {
    data: Vec<T>,
}

impl<T> ActorModelComponent<T> {
    pub fn get(&self, model: &ActorModel) -> &T {
        &self.data[model.as_usize()]
    }
}

impl<T> Initialization for ActorModelComponent<T>
where
    T: FromDescriptor + Default + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let mut vec = Vec::new();

        let label_map = world
            .get_resource_ref::<LabelLibrary>()
            .get_label_map_for::<ActorModel>()
            .ok_or_else(|| anyhow::anyhow!("ActorModel label map is undefined"))?
            .clone();
        let component_map = world.get_resource_ref::<ComponentMap<ActorModel>>();

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

        Ok(Self { data: vec })
    }
}
