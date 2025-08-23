use crate::{
    read_data_file,
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

type ComponentMapDescriptor = HashMap<String, Box<RawValue>>;

pub struct ComponentMap<E> {
    label_map: LabelMap<E>,
    components: HashMap<String, Vec<Option<Box<RawValue>>>>,
}

impl<E> ComponentMap<E>
where
    E: AsFromUsize + Send + Sync + 'static,
{
    /// Loads components of an entity files in the specified directory.
    pub async fn load_data(
        path: impl AsRef<Path>,
        label_library: &LabelLibrary,
    ) -> Result<Self, Error> {
        let path = path.as_ref().to_owned();
        let label_map = label_library
            .get_label_map_for::<E>()
            .expect("label map undefined");

        task::spawn_blocking(move || {
            let mut components = HashMap::new();

            for (entity, entity_label) in label_map.iter() {
                let file_name = format!("{}.json", entity_label);

                let descriptor: ComponentMapDescriptor = read_data_file(path.join(file_name))?;

                for (component_label, component_value) in descriptor.into_iter() {
                    let component_vec = match components.get_mut(&component_label) {
                        Some(c) => c,
                        None => {
                            components.insert(component_label.clone(), vec![None; label_map.len()]);

                            components.get_mut(&component_label).unwrap()
                        },
                    };

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

    pub fn get_component<'a, 'b, D>(
        &'a self,
        component_label: &'b str,
    ) -> impl Iterator<Item = Result<(E, D), Error>> + 'a
    where
        'b: 'a,
        D: Deserialize<'a>,
    {
        self.components
            .get(component_label)
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(idx, val)| Some((E::from_usize(idx), val.as_ref()?)))
            .map(move |(entity, val)| {
                let descriptor = serde_json::from_str::<D>(val.get()).with_context(|| {
                    let label = self.label_map.get_label(&entity).unwrap();

                    format!(
                        "unable to parse component \"{}\" of entity \"{}\"",
                        component_label, label
                    )
                })?;

                Ok((entity, descriptor))
            })
    }
}
