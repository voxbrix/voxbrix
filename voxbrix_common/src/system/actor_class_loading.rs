use crate::{
    assets::{
        ACTOR_CLASS_DIR,
        ACTOR_CLASS_LIST_PATH,
    },
    entity::actor_class::ActorClass,
    read_data_file,
    LabelMap,
};
use anyhow::Error;
use serde::{
    de::DeserializeOwned,
    Deserialize,
};
use serde_json::value::RawValue;
use std::{
    collections::BTreeMap,
    path::Path,
};
use tokio::task;

pub trait LoadActorClassComponent<T> {
    fn reload_classes(&mut self, data: Vec<Option<T>>);
}

#[derive(Deserialize, Debug)]
struct ActorClassList {
    list: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct ActorClassDescriptior {
    label: String,
    components: BTreeMap<String, Box<RawValue>>,
}

pub struct ActorClassLoadingSystem {
    actor_class_list: Vec<String>,
    components: BTreeMap<String, Vec<Option<Box<RawValue>>>>,
}

impl ActorClassLoadingSystem {
    pub async fn load_data() -> Result<Self, Error> {
        task::spawn_blocking(|| {
            let actor_class_list = read_data_file::<ActorClassList>(ACTOR_CLASS_LIST_PATH)?.list;

            let mut components = BTreeMap::new();

            for (actor_class_id, actor_class_label) in actor_class_list.iter().enumerate() {
                let file_name = format!("{}.json", actor_class_label);

                let descriptor: ActorClassDescriptior =
                    read_data_file(Path::new(ACTOR_CLASS_DIR).join(file_name))?;

                if descriptor.label != *actor_class_label {
                    return Err(Error::msg(format!(
                        "Label defined in file differs from file name: {} in {}.json",
                        descriptor.label, actor_class_label
                    )));
                }

                for (component_label, component_value) in descriptor.components.into_iter() {
                    let component_vec = match components.get_mut(&component_label) {
                        Some(c) => c,
                        None => {
                            components.insert(
                                component_label.clone(),
                                vec![None; actor_class_list.len()],
                            );

                            components.get_mut(&component_label).unwrap()
                        },
                    };

                    component_vec[actor_class_id] = Some(component_value);
                }
            }

            Ok(Self {
                actor_class_list,
                components,
            })
        })
        .await
        .unwrap()
    }

    pub fn load_component<D, C>(
        &self,
        component_label: &str,
        component: &mut impl LoadActorClassComponent<C>,
        conversion: impl Fn(D) -> Result<C, Error>,
    ) -> Result<(), Error>
    where
        D: DeserializeOwned,
    {
        let data = self
            .components
            .get(component_label)
            .unwrap_or(&Vec::new())
            .into_iter()
            .map(|val_opt| {
                val_opt
                    .as_ref()
                    .map(|val| {
                        let descriptor = serde_json::from_str::<D>(val.get())?;

                        conversion(descriptor)
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;

        component.reload_classes(data);

        Ok(())
    }

    pub fn into_label_map(&self) -> LabelMap<ActorClass> {
        LabelMap::from_list(&self.actor_class_list)
    }
}
