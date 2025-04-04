use crate::{
    assets::{
        BLOCK_CLASS_DIR,
        BLOCK_CLASS_LIST_PATH,
    },
    component::block_class::BlockClassComponent,
    entity::block_class::BlockClass,
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

#[derive(Deserialize, Debug)]
struct BlockClassList {
    list: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct BlockClassDescriptior {
    label: String,
    components: BTreeMap<String, Box<RawValue>>,
}

pub struct BlockClassLoadingSystem {
    block_class_list: Vec<String>,
    components: BTreeMap<String, Vec<Option<Box<RawValue>>>>,
}

impl BlockClassLoadingSystem {
    pub async fn load_data() -> Result<Self, Error> {
        task::spawn_blocking(|| {
            let block_class_list = read_data_file::<BlockClassList>(BLOCK_CLASS_LIST_PATH)?.list;

            let mut components = BTreeMap::new();

            for (block_class_id, block_class_label) in block_class_list.iter().enumerate() {
                let file_name = format!("{}.json", block_class_label);

                let descriptor: BlockClassDescriptior =
                    read_data_file(Path::new(BLOCK_CLASS_DIR).join(file_name))?;

                if descriptor.label != *block_class_label {
                    return Err(Error::msg(format!(
                        "Label defined in file differs from file name: {} in {}.json",
                        descriptor.label, block_class_label
                    )));
                }

                for (component_label, component_value) in descriptor.components.into_iter() {
                    let component_vec = match components.get_mut(&component_label) {
                        Some(c) => c,
                        None => {
                            components.insert(
                                component_label.clone(),
                                vec![None; block_class_list.len()],
                            );

                            components.get_mut(&component_label).unwrap()
                        },
                    };

                    component_vec[block_class_id] = Some(component_value);
                }
            }

            Ok(Self {
                block_class_list,
                components,
            })
        })
        .await
        .unwrap()
    }

    pub fn load_component<D, C, F>(
        &self,
        component_label: &str,
        component: &mut BlockClassComponent<C>,
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

        component.reload(data);

        Ok(())
    }

    pub fn into_label_map(self) -> LabelMap<BlockClass> {
        LabelMap::from_list(&self.block_class_list)
    }
}
