pub mod assets;
pub mod async_ext;
pub mod component;
pub mod entity;
pub mod math;
pub mod messages;
pub mod pack;
pub mod resource;
pub mod script_convert;
pub mod script_registry;
pub mod system;

use ahash::AHashMap;
use anyhow::Context;
use arrayvec::ArrayVec;
use component::block::{
    metadata::BlockMetadata,
    BlocksVec,
};
use entity::{
    block_class::BlockClass,
    block_environment::BlockEnvironment,
    chunk::Chunk,
};
use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};
use std::{
    any::{
        Any,
        TypeId,
    },
    fs,
    path::Path,
    sync::Arc,
};
use tokio::task;

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
pub fn read_data_file<T>(path: impl AsRef<Path> + std::fmt::Debug) -> Result<T, anyhow::Error>
where
    T: DeserializeOwned,
{
    let string =
        fs::read_to_string(path.as_ref()).with_context(|| format!("reading {:?}", &path))?;
    let data =
        serde_json::from_str::<T>(&string).with_context(|| format!("parsing {:?}", &path))?;

    Ok(data)
}

pub async fn read_file_async(
    path: impl AsRef<Path> + std::fmt::Debug + Send + 'static,
) -> Result<Vec<u8>, anyhow::Error> {
    task::spawn_blocking(move || {
        fs::read(path.as_ref()).with_context(|| format!("reading {:?}", &path))
    })
    .await
    .expect("unable to join blocking task")
}

pub trait AsFromUsize {
    fn as_usize(&self) -> usize;
    fn from_usize(i: usize) -> Self;
}

impl AsFromUsize for usize {
    fn as_usize(&self) -> usize {
        (*self).try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        i.try_into().unwrap()
    }
}

impl AsFromUsize for u32 {
    fn as_usize(&self) -> usize {
        (*self).try_into().unwrap()
    }

    fn from_usize(i: usize) -> Self {
        i.try_into().unwrap()
    }
}

#[derive(Debug)]
struct LabelMapInner<T> {
    labels: Vec<Arc<str>>,
    entities: AHashMap<Arc<str>, T>,
}

#[derive(Clone, Debug)]
pub struct LabelMap<T>(Arc<LabelMapInner<T>>);

impl<T> LabelMap<T>
where
    T: AsFromUsize,
{
    pub fn get_label(&self, entity: &T) -> Option<&str> {
        self.0.labels.get(entity.as_usize()).map(|l| l.as_ref())
    }

    pub fn len(&self) -> usize {
        self.0.labels.len()
    }
}

impl<T> LabelMap<T>
where
    T: Copy,
{
    pub fn get(&self, label: &str) -> Option<T> {
        self.0.entities.get(label).copied()
    }
}

impl<T> LabelMap<T>
where
    T: AsFromUsize,
{
    pub fn from_list(list: &[String]) -> Self {
        let labels: Vec<Arc<str>> = list.iter().map(|s| s.as_str().into()).collect();
        let entities = labels
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, l)| (l, T::from_usize(i)))
            .collect();

        Self(Arc::new(LabelMapInner { labels, entities }))
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (T, &str)> {
        self.0
            .labels
            .iter()
            .enumerate()
            .map(|(i, l)| (T::from_usize(i), l.as_ref()))
    }
}

#[derive(Debug)]
struct LabelLibraryInner(AHashMap<TypeId, Arc<dyn Any + Send + Sync>>);

#[derive(Clone, Debug)]
pub struct LabelLibrary(Arc<LabelLibraryInner>);

impl LabelLibrary {
    pub fn new() -> Self {
        Self(Arc::new(LabelLibraryInner(AHashMap::new())))
    }

    pub fn add_label_map<T>(&mut self, label_map: LabelMap<T>)
    where
        T: Any + Send + Sync,
    {
        Arc::get_mut(&mut self.0)
            .expect("cannot add label map: library was already cloned")
            .0
            .insert(TypeId::of::<T>(), label_map.0);
    }

    pub async fn load<T>(&mut self, path: impl AsRef<Path>) -> Result<(), anyhow::Error>
    where
        T: AsFromUsize + Any + Send + Sync,
    {
        let read_path = path.as_ref().to_owned();

        let list = task::spawn_blocking(move || read_data_file::<Vec<String>>(read_path))
            .await
            .unwrap()
            .with_context(|| {
                format!(
                    "unable to load list \"{:?}\"",
                    path.as_ref().to_string_lossy()
                )
            })?;

        let label_map = LabelMap::<T>::from_list(&list);

        Arc::get_mut(&mut self.0)
            .expect("cannot add label map: library was already cloned")
            .0
            .insert(TypeId::of::<T>(), label_map.0);

        Ok(())
    }

    pub fn get_label_map_for<T>(&self) -> Option<LabelMap<T>>
    where
        T: Send + Sync + 'static,
    {
        let map = self
            .0
             .0
            .get(&TypeId::of::<T>())?
            .clone()
            .downcast::<LabelMapInner<T>>()
            .expect("incorrect label map boxing");

        Some(LabelMap(map))
    }

    pub fn get_label<T>(&self, entity: &T) -> Option<&str>
    where
        T: AsFromUsize + 'static,
    {
        let map = self
            .0
             .0
            .get(&TypeId::of::<T>())?
            .downcast_ref::<LabelMapInner<T>>()
            .expect("incorrect label map boxing");

        map.labels.get(entity.as_usize()).map(|l| l.as_ref())
    }
}

impl LabelLibrary {
    pub fn get<T>(&self, label: &str) -> Option<T>
    where
        T: Copy + 'static,
    {
        let map = self
            .0
             .0
            .get(&TypeId::of::<T>())?
            .downcast_ref::<LabelMapInner<T>>()
            .expect("incorrect label map boxing");

        map.entities.get(label).copied()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChunkData {
    pub chunk: Chunk,
    pub block_classes: BlocksVec<BlockClass>,
    pub block_environment: BlocksVec<BlockEnvironment>,
    pub block_metadata: BlocksVec<BlockMetadata>,
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
