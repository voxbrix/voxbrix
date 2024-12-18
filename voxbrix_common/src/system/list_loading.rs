use crate::{
    read_data_file,
    AsFromUsize,
    LabelMap,
};
use anyhow::{
    Context,
    Error,
};
use serde::Deserialize;
use std::{
    fmt::Debug,
    path::Path,
};
use tokio::task;

#[derive(Deserialize, Clone, Debug)]
pub struct List {
    pub list: Vec<String>,
}

impl List {
    pub async fn load(
        path: impl 'static + AsRef<Path> + Debug + Send + Clone,
    ) -> Result<Self, Error> {
        let read_path = path.clone();

        task::spawn_blocking(move || read_data_file::<List>(read_path))
            .await
            .unwrap()
            .with_context(|| format!("unable to load list \"{:?}\"", path))
    }

    pub fn into_label_map<T>(&self) -> LabelMap<T>
    where
        T: AsFromUsize,
    {
        LabelMap::from_list(&self.list)
    }
}
