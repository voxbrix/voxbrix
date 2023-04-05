use crate::{
    component::block_class::BlockClassComponent,
    entity::block_class::BlockClass,
};
use anyhow::Error;
use ron::Value;
use serde::{
    de::DeserializeOwned,
    Deserialize,
};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
};

const PATH: &str = "assets/block_classes";
const LIST_FILE_NAME: &str = "list.ron";

pub struct BlockClassLoadingSystem {
    block_class_list: Vec<String>,
    components: BTreeMap<String, Vec<Option<Value>>>,
}

impl BlockClassLoadingSystem {
    pub async fn load_data() -> Result<Self, Error> {
        blocking::unblock(|| {
            let block_class_list = {
                let path = Path::new(PATH).join(LIST_FILE_NAME);
                let string = fs::read_to_string(path)?;
                ron::from_str::<BlockClassList>(&string)?.list
            };

            let mut components = BTreeMap::new();

            for (block_class_id, block_class_label) in block_class_list.iter().enumerate() {
                let file_name = format!("{}.ron", block_class_label);
                let path = Path::new(PATH).join(file_name);
                let string = fs::read_to_string(path)?;

                let descriptor: BlockClassDescriptior = ron::from_str(&string)?;

                if descriptor.label != *block_class_label {
                    return Err(Error::msg(format!(
                        "Label defined in file differs from file name: {} in {}.ron",
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
    }

    pub fn load_component<T>(
        &self,
        component_label: &str,
        component: &mut BlockClassComponent<T>,
    ) -> Result<(), Error>
    where
        T: DeserializeOwned,
    {
        let data = self
            .components
            .get(component_label)
            .unwrap_or(&Vec::new())
            .into_iter()
            .filter_map(|x| x.as_ref())
            .map(|val| val.clone().into_rust())
            .collect::<Result<Vec<_>, _>>()?;

        component.reload(data);

        Ok(())
    }

    pub fn into_label_map(self) -> BTreeMap<String, BlockClass> {
        self.block_class_list
            .into_iter()
            .enumerate()
            .map(|(c, l)| (l, BlockClass(c)))
            .collect()
    }
}

#[derive(Deserialize, Debug)]
struct BlockClassList {
    list: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct BlockClassDescriptior {
    label: String,
    components: BTreeMap<String, Value>,
}
