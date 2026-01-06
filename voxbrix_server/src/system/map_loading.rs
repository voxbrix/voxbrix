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
use voxbrix_common::parse_file_async;

#[derive(Deserialize, Clone, Debug)]
pub struct Map<T>(HashMap<String, T>);

impl<T> Map<T>
where
    T: DeserializeOwned + Send + 'static,
{
    pub async fn load(path: impl AsRef<Path> + Debug) -> Result<Self, Error> {
        parse_file_async::<Self>(path.as_ref())
            .await
            .with_context(|| format!("unable to load map \"{:?}\"", path))
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.0.get(key)
    }
}
