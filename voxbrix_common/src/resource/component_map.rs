use crate::{
    parse_file,
    AsFromUsize,
    LabelLibrary,
    LabelMap,
};
use anyhow::{
    Context,
    Error,
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{
    collections::HashMap,
    path::Path,
};
use tokio::task;
use voxbrix_world::{
    Initialization,
    World,
};

type ComponentMapDescriptor = HashMap<String, Box<RawValue>>;

pub struct ComponentMap<E> {
    label_map: LabelMap<E>,
    components: HashMap<String, Vec<Option<Box<RawValue>>>>,
}

impl<E> ComponentMap<E>
where
    E: AsFromUsize + Send + Sync + 'static,
{
    pub fn get_component<'a, 'b, D>(
        &'a self,
        component_label: &'b str,
    ) -> impl Iterator<Item = Result<(E, Option<D>), Error>> + 'a
    where
        'b: 'a,
        D: Deserialize<'a>,
    {
        let component = self.components.get(component_label);

        self.label_map.iter().map(move |(entity, _)| {
            let comp = component
                .and_then(|c| c.get(entity.as_usize()).and_then(|o| o.as_ref()))
                .map(|val| {
                    serde_json::from_str::<D>(val.get()).with_context(|| {
                        let label = self.label_map.get_label(&entity).unwrap();

                        format!(
                            "unable to parse component \"{}\" of entity \"{}\"",
                            component_label, label
                        )
                    })
                })
                .transpose()?;

            Ok((entity, comp))
        })
    }
}

pub trait ComponentMapEntity: AsFromUsize + Send + Sync + 'static {
    const COMPONENT_MAP_DIR: &str;
}

impl<E> Initialization for ComponentMap<E>
where
    E: ComponentMapEntity,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let label_map = world
            .get_resource_ref::<LabelLibrary>()
            .get_label_map_for::<E>()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "label map for component map directory \"{}\" is undefined",
                    E::COMPONENT_MAP_DIR,
                )
            })?;

        task::spawn_blocking(move || {
            let mut components = HashMap::new();

            for (entity, entity_label) in label_map.iter() {
                let file_name = format!("{}.json", entity_label);
                let dir: &Path = E::COMPONENT_MAP_DIR.as_ref();

                let descriptor: ComponentMapDescriptor = parse_file(dir.join(file_name))?;

                for (component_label, component_value) in descriptor.into_iter() {
                    let component_vec = components
                        .entry(component_label)
                        .or_insert_with(|| vec![None; label_map.len()]);

                    component_vec[entity.as_usize()] = Some(component_value);
                }
            }

            Ok(Self {
                label_map,
                components,
            })
        })
        .await
        .unwrap()
    }
}
