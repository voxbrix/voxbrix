use crate::{
    entity::actor_model::ActorModel,
    system::model_loading::LoadableComponent,
};
use ron::Value;
use serde::Deserialize;
use std::collections::BTreeMap;

pub mod builder;

pub struct ActorModelComponent<T> {
    data: Vec<Option<T>>,
}

impl<T> ActorModelComponent<T> {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn get(&self, i: ActorModel) -> Option<&T> {
        self.data.get(i.0)?.as_ref()
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
    components: BTreeMap<String, Value>,
}
