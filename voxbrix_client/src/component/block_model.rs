use crate::entity::block_model::BlockModel;
use std::collections::BTreeMap;
use serde::Deserialize;
use ron::Value;

pub mod builder;
pub mod culling;

pub struct BlockModelComponent<T> {
    data: Vec<Option<T>>,
}

impl<T> BlockModelComponent<T> {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
        }
    }

    pub fn get(&self, i: BlockModel) -> Option<&T> {
        self.data.get(i.0)?.as_ref()
    }

    pub fn reload(&mut self, data: Vec<Option<T>>) {
        self.data = data;
    }
}

#[derive(Deserialize, Debug)]
pub struct BlockModelDescriptor {
    label: String,
    components: BTreeMap<String, Value>,
}


