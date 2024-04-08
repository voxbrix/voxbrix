pub mod assets;
pub mod async_ext;
pub mod component;
pub mod entity;
pub mod math;
pub mod messages;
pub mod pack;
pub mod script_convert;
pub mod script_registry;
pub mod sparse_vec;
pub mod system;

use anyhow::Context;
use arrayvec::ArrayVec;
use bincode::{
    Decode,
    Encode,
};
use component::block::BlocksVec;
use entity::{
    block_class::BlockClass,
    chunk::Chunk,
};
use serde::de::DeserializeOwned;
use std::{
    collections::BTreeMap,
    fs,
    iter::FromIterator,
    path::Path,
};

/// Moves the block with the data in the brackets into the rayon threadpool and awaits for the data
/// to be returned.
#[macro_export]
macro_rules! compute {
    (($($a:ident),+)$e:expr) => {
        {
            let (task_output_tx, task_output_rx) = flume::bounded(1);
            let res;

            rayon::spawn(move || {
                let res = $e;
                task_output_tx.try_send((($($a),+), res)).unwrap();
            });

            (($($a),+), res) = task_output_rx.recv_async().await.unwrap();

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

#[derive(Clone, Debug)]
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

#[derive(Encode, Decode, Clone)]
pub struct ChunkData {
    pub chunk: Chunk,
    pub block_classes: BlocksVec<BlockClass>,
}

pub trait ArrayExt<T, const N: usize> {
    fn map_ref<F, U>(&self, f: F) -> [U; N]
    where
        F: FnMut(&T) -> U;
}

impl<T, const N: usize> ArrayExt<T, N> for [T; N] {
    fn map_ref<F, U>(&self, f: F) -> [U; N]
    where
        F: FnMut(&T) -> U,
    {
        unsafe {
            self.iter()
                .map(f)
                .collect::<ArrayVec<_, N>>()
                .into_inner_unchecked()
        }
    }
}
