use flume::Sender;
use redb::{
    Key,
    Value,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    cmp::Ordering,
    fmt::Debug,
    marker::PhantomData,
    thread,
};
use voxbrix_common::pack::{
    Pack,
    Packer,
};

pub struct StorageThread {
    tx: Sender<Box<dyn FnMut() + Send>>,
}

impl StorageThread {
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded::<Box<dyn FnMut() + Send>>();
        thread::spawn(move || {
            while let Ok(mut task) = rx.recv() {
                task();
            }
        });

        Self { tx }
    }

    pub fn execute<F>(&self, task: F)
    where
        F: 'static + FnMut() + Send,
    {
        let _ = self.tx.send(Box::new(task));
    }
}

#[derive(Debug)]
pub struct DataSized<T>(T);

impl<T> DataSized<T>
where
    T: IntoDataSized,
{
    pub fn into_inner(self) -> T {
        self.0
    }

    pub fn from_inner(value: T) -> Self {
        Self(value)
    }
}

pub trait Array: AsRef<[u8]> {
    const SIZE: usize;

    fn from_slice(slice: &[u8]) -> Self;
}

impl<const SIZE: usize> Array for [u8; SIZE] {
    const SIZE: usize = SIZE;

    fn from_slice(slice: &[u8]) -> Self {
        slice.try_into().expect("slice must have correct length")
    }
}

pub trait IntoDataSized: TypeName {
    type Array: Array;
    const SIZE: usize = Self::Array::SIZE;

    fn to_bytes(&self) -> Self::Array;
    fn from_bytes(bytes: &Self::Array) -> Self;

    fn into_data_sized(self) -> DataSized<Self>
    where
        Self: Sized,
    {
        DataSized::from_inner(self)
    }

    fn from_data_sized(value: DataSized<Self>) -> Self
    where
        Self: Sized,
    {
        value.0
    }
}

impl<T> Value for DataSized<T>
where
    T: IntoDataSized + Debug,
{
    type AsBytes<'a>
        = T::Array
    where
        Self: 'a;
    type SelfType<'a>
        = DataSized<T>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(T::Array::SIZE)
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        DataSized(T::from_bytes(&T::Array::from_slice(data)))
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a + 'b,
    {
        value.0.to_bytes()
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new(T::NAME)
    }
}

impl<T> Key for DataSized<T>
where
    T: Ord + IntoDataSized + Debug,
{
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        data1.cmp(&data2)
    }
}

#[derive(Debug)]
pub struct UnstoreError;

#[derive(Debug)]
enum DataContainer<'a> {
    Shared(&'a [u8]),
    Owned(Vec<u8>),
}

impl<'a> AsRef<[u8]> for DataContainer<'a> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Shared(r) => r,
            Self::Owned(d) => d.as_slice(),
        }
    }
}

pub trait TypeName {
    const NAME: &'static str;
}

/// Typed byte chunk for storage purposes. Implements `AsRef<u8>`, so you could deserialize it to
/// concrete type.
#[derive(Debug)]
pub struct Data<'a, T> {
    kind: PhantomData<T>,
    data: DataContainer<'a>,
}

impl<'a, T> Data<'a, T> {
    pub fn new_shared(data: &'a [u8]) -> Data<'a, T> {
        Self {
            kind: PhantomData::<T>,
            data: DataContainer::Shared(data),
        }
    }
}

impl<T> Data<'static, T> {
    pub fn new_owned(data: Vec<u8>) -> Self {
        Self {
            kind: PhantomData::<T>,
            data: DataContainer::Owned(data),
        }
    }
}

impl<'a, T> Data<'a, T>
where
    T: Pack + Deserialize<'a>,
{
    pub fn into_inner(&'a self, packer: &'a mut Packer) -> T {
        packer.unpack(self.data.as_ref()).unwrap()
    }
}

impl<T> Data<'static, T>
where
    T: Pack + Serialize,
{
    pub fn from_inner(value: &T, packer: &mut Packer) -> Self {
        Self::new_owned(packer.pack_to_vec(value))
    }
}

pub trait IntoData {
    fn into_data(&self, packer: &mut Packer) -> Data<'static, Self>
    where
        Self: Pack + Serialize + Sized,
    {
        Data::from_inner(self, packer)
    }
}

impl<T> IntoData for T where T: Pack + Serialize {}

impl<T> Value for Data<'_, T>
where
    T: TypeName + Debug,
{
    type AsBytes<'a>
        = &'a [u8]
    where
        Self: 'a;
    type SelfType<'a>
        = Data<'a, T>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        Data::new_shared(data)
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a + 'b,
    {
        value.data.as_ref()
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new(T::NAME)
    }
}

pub mod player {
    use crate::storage::TypeName;
    use serde::{
        Deserialize,
        Serialize,
    };
    use voxbrix_common::pack::Pack;

    #[derive(Serialize, Deserialize, Debug)]
    pub struct PlayerProfile {
        pub username: String,
        #[serde(with = "serde_big_array::BigArray")]
        pub public_key: [u8; 33],
    }

    impl Pack for PlayerProfile {
        const DEFAULT_COMPRESSED: bool = false;
    }

    impl TypeName for PlayerProfile {
        const NAME: &'static str = "PlayerProfile";
    }
}
