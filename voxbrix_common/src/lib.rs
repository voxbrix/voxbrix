pub mod async_ext;
pub mod component;
pub mod entity;
pub mod math;
pub mod messages;
pub mod pack;
pub mod sparse_vec;
pub mod system;

use anyhow::Context;
use component::block::BlocksVec;
use entity::{
    block_class::BlockClass,
    chunk::Chunk,
};
use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};
use std::{
    collections::BTreeMap,
    fs,
    iter::FromIterator,
    path::Path,
};

#[macro_export]
macro_rules! unblock {
    (($($a:ident),+)$e:expr) => {
        {
            let res;

            (($($a),+), res) = tokio::task::spawn_blocking(move || {
                let res = $e;
                (($($a),+), res)
            }).await.unwrap();

            res
        }
    };
}

/// Blocking IO, must not be used directly in async
pub fn read_ron_file<T>(path: impl AsRef<Path> + std::fmt::Debug) -> Result<T, anyhow::Error>
where
    T: DeserializeOwned,
{
    let string =
        fs::read_to_string(path.as_ref()).with_context(|| format!("reading {:?}", &path))?;
    let data = ron::from_str::<T>(&string).with_context(|| format!("parsing {:?}", &path))?;

    Ok(data)
}

#[derive(Debug)]
pub struct LabelMap<T>(BTreeMap<String, T>);

impl<T> LabelMap<T>
where
    T: Copy,
{
    pub fn get(&self, label: &str) -> Option<T> {
        self.0.get(label).copied()
    }
}

impl<T> From<BTreeMap<String, T>> for LabelMap<T> {
    fn from(value: BTreeMap<String, T>) -> Self {
        Self(value)
    }
}

impl<A> FromIterator<(String, A)> for LabelMap<A> {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (String, A)>,
    {
        LabelMap(BTreeMap::from_iter(iter))
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChunkData {
    pub chunk: Chunk,
    pub block_classes: BlocksVec<BlockClass>,
}
