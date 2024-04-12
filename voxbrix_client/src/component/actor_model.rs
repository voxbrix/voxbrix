use crate::system::model_loading::LoadableComponent;
use ron::value::RawValue;
use serde::Deserialize;
use std::collections::BTreeMap;
use voxbrix_common::entity::actor_model::ActorModel;

pub mod builder;

pub struct ActorModelComponent<T> {
    data: Vec<Option<T>>,
}

impl<T> ActorModelComponent<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn get(&self, model: &ActorModel) -> Option<&T> {
        self.data.get(model.into_usize())?.as_ref()
    }
}

impl<T> LoadableComponent<T> for ActorModelComponent<T> {
    fn reload(&mut self, data: Vec<Option<T>>) {
        self.data = data;
    }
}

#[derive(Deserialize, Debug)]
pub struct ActorModelDescriptor {
    label: String,
    components: BTreeMap<String, Box<RawValue>>,
}
