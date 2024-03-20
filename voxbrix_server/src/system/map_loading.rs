use anyhow::{
    Context,
    Error,
};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fmt::Debug,
    path::Path,
};
use tokio::task;
use voxbrix_common::read_ron_file;

#[derive(Deserialize, Clone, Debug)]
pub struct Map {
    pub map: HashMap<String, String>,
}

impl Map {
    pub async fn load(
        path: impl 'static + AsRef<Path> + Debug + Send + Clone,
    ) -> Result<Self, Error> {
        let read_path = path.clone();

        task::spawn_blocking(move || read_ron_file::<Map>(read_path))
            .await
            .unwrap()
            .with_context(|| format!("unable to load map \"{:?}\"", path))
    }

    pub fn iter<'a>(&'a self) -> impl ExactSizeIterator<Item = (&'a str, &'a str)> {
        self.map.iter().map(|(a, s)| (a.as_str(), s.as_str()))
    }
}
