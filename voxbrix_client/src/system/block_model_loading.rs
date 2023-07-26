use crate::entity::block_model::BlockModel;
use crate::component::block_model::BlockModelComponent;
use anyhow::Error;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::Path,
};
use tokio::task;
use voxbrix_common::{
    read_ron_file,
    LabelMap,
};
use ron::Value;
use serde::de::DeserializeOwned;

const MODELS_PATH: &str = "assets/client/models/blocks";
const MODEL_LIST_PATH: &str = "assets/client/models/blocks.ron";

#[derive(Deserialize, Debug)]
struct List {
    list: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct BlockModelDescriptior {
    label: String,
    components: BTreeMap<String, Value>,
}

pub struct BlockModelLoadingSystem {
    block_model_list: Vec<String>,
    components: BTreeMap<String, Vec<Option<Value>>>,
}

impl BlockModelLoadingSystem {
    pub async fn load_data() -> Result<Self, Error> {
        task::spawn_blocking(|| {
            let block_model_list = read_ron_file::<List>(MODEL_LIST_PATH)?.list;

            let mut components = BTreeMap::new();

            for (block_model_id, block_model_label) in block_model_list.iter().enumerate() {
                let file_name = format!("{}.ron", block_model_label);

                let descriptor: BlockModelDescriptior =
                    read_ron_file(Path::new(MODELS_PATH).join(file_name))?;

                if descriptor.label != *block_model_label {
                    return Err(Error::msg(format!(
                        "Label defined in file differs from file name: {} in {}.ron",
                        descriptor.label, block_model_label
                    )));
                }

                for (component_label, component_value) in descriptor.components.into_iter() {
                    let component_vec = match components.get_mut(&component_label) {
                        Some(c) => c,
                        None => {
                            components.insert(
                                component_label.clone(),
                                vec![None; block_model_list.len()],
                            );

                            components.get_mut(&component_label).unwrap()
                        },
                    };

                    component_vec[block_model_id] = Some(component_value);
                }
            }

            Ok(Self {
                block_model_list,
                components,
            })
        })
        .await
        .unwrap()
    }

    pub fn load_component<D, C, F>(
        &self,
        component_label: &str,
        component: &mut BlockModelComponent<C>,
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
                        let descriptor = val.clone().into_rust::<D>()?;

                        conversion(descriptor)
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;

        component.reload(data);

        Ok(())
    }

    pub fn into_label_map(self) -> LabelMap<BlockModel> {
        self.block_model_list
            .into_iter()
            .enumerate()
            .map(|(c, l)| (l, BlockModel(c)))
            .collect()
    }
}
