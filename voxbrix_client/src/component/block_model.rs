use crate::{
    entity::block_model::BlockModel,
    system::model_loading::LoadableComponent,
};
use ron::value::RawValue;
use serde::Deserialize;
use std::collections::BTreeMap;
use voxbrix_common::AsFromUsize;

pub mod builder;
pub mod culling;

pub struct BlockModelComponent<T> {
    data: Vec<Option<T>>,
}

impl<T> BlockModelComponent<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn get(&self, block_model: &BlockModel) -> Option<&T> {
        self.data.get(block_model.as_usize())?.as_ref()
    }
}

impl<T> LoadableComponent<T> for BlockModelComponent<T> {
    fn reload(&mut self, data: Vec<Option<T>>) {
        self.data = data;
    }
}

#[derive(Deserialize, Debug)]
pub struct BlockModelDescriptor {
    label: String,
    components: BTreeMap<String, Box<RawValue>>,
}
