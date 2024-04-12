use anyhow::{
    Context,
    Error,
};
use ron::value::RawValue;
use serde::{
    de::DeserializeOwned,
    Deserialize,
};
use std::{
    collections::BTreeMap,
    path::Path,
};
use tokio::task;
use voxbrix_common::{
    read_ron_file,
    system::list_loading::List,
    LabelMap,
};

pub trait LoadableComponent<C> {
    fn reload(&mut self, data: Vec<Option<C>>);
}

#[derive(Deserialize, Debug)]
struct ModelDescriptior {
    label: String,
    components: BTreeMap<String, Box<RawValue>>,
}

pub struct ModelLoadingSystem {
    model_list: Vec<String>,
    components: BTreeMap<String, Vec<Option<Box<RawValue>>>>,
}

impl ModelLoadingSystem {
    pub async fn load_data(
        list_path: &'static str,
        path_prefix: &'static str,
    ) -> Result<Self, Error> {
        task::spawn_blocking(move || {
            let model_list = read_ron_file::<List>(list_path)?.list;

            let mut components = BTreeMap::new();

            for (model_id, model_label) in model_list.iter().enumerate() {
                let file_name = format!("{}.ron", model_label);

                let descriptor: ModelDescriptior =
                    read_ron_file(Path::new(path_prefix).join(file_name))?;

                if descriptor.label != *model_label {
                    return Err(Error::msg(format!(
                        "label defined in file differs from file name: {} in {}.ron",
                        descriptor.label, model_label
                    )));
                }

                for (component_label, component_value) in descriptor.components.into_iter() {
                    let component_vec = match components.get_mut(&component_label) {
                        Some(c) => c,
                        None => {
                            components
                                .insert(component_label.clone(), vec![None; model_list.len()]);

                            components.get_mut(&component_label).unwrap()
                        },
                    };

                    component_vec[model_id] = Some(component_value);
                }
            }

            Ok(Self {
                model_list,
                components,
            })
        })
        .await
        .unwrap()
    }

    pub fn load_component<D, C, F>(
        &self,
        component_label: &str,
        component: &mut impl LoadableComponent<C>,
        conversion: F,
    ) -> Result<(), Error>
    where
        D: DeserializeOwned,
        F: Fn(D) -> Result<C, Error>,
    {
        let data = self
            .components
            .get(component_label)
            .unwrap_or(&Vec::new())
            .into_iter()
            .enumerate()
            .map(|(model_idx, val_opt)| {
                val_opt
                    .as_ref()
                    .map(|val| {
                        let descriptor = val.clone().into_rust::<D>()?;

                        conversion(descriptor)
                    })
                    .transpose()
                    .with_context(|| {
                        format!("model \"{}\"", self.model_list.get(model_idx).unwrap())
                    })
            })
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("model component \"{}\"", component_label))?;

        component.reload(data);

        Ok(())
    }

    pub fn into_label_map<E>(self, f: impl Fn(usize) -> E) -> LabelMap<E> {
        self.model_list
            .into_iter()
            .enumerate()
            .map(|(c, l)| (l, f(c)))
            .collect()
    }
}
