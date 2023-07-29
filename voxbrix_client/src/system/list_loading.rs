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
use voxbrix_common::{
    read_ron_file,
    LabelMap,
};

#[derive(Deserialize, Debug)]
pub struct List {
    pub list: Vec<String>,
}

impl List {
    pub async fn load(
        path: impl 'static + AsRef<Path> + Debug + Send + Clone,
    ) -> Result<Self, Error> {
        let read_path = path.clone();

        task::spawn_blocking(move || read_ron_file::<List>(read_path))
            .await
            .unwrap()
            .with_context(|| format!("unable to load list \"{:?}\"", path))
    }

    pub fn into_label_map<E>(self, f: impl Fn(usize) -> E) -> LabelMap<E> {
        self.list
            .into_iter()
            .enumerate()
            .map(|(i, label)| (label, f(i)))
            .collect()
    }
}
