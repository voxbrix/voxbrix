use anyhow::{
    Context,
    Error,
};
use serde::{
    de::DeserializeOwned,
    Deserialize,
};
use std::{
    collections::HashMap,
    fmt::Debug,
    path::Path,
};
use tokio::task;
use voxbrix_common::read_data_file;

#[derive(Deserialize, Clone, Debug)]
pub struct Map<T>(HashMap<String, T>);

impl<T> Map<T>
where
    T: DeserializeOwned + Send + 'static,
{
    pub async fn load(
        path: impl 'static + AsRef<Path> + Debug + Send + Clone,
    ) -> Result<Self, Error> {
        let read_path = path.clone();

        task::spawn_blocking(move || read_data_file::<Self>(read_path))
            .await
            .unwrap()
            .with_context(|| format!("unable to load map \"{:?}\"", path))
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.0.get(key)
    }
}
