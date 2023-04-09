use flume::Sender;
use lz4_flex::block as lz4;
use postcard::Error;
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::{
    marker::PhantomData,
    mem,
    thread,
};

pub struct StorageThread {
    tx: Sender<Box<dyn FnMut(&mut Vec<u8>) + Send>>,
}

impl StorageThread {
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded::<Box<dyn FnMut(&mut Vec<u8>) + Send>>();
        thread::spawn(move || {
            // Shared buffer to serialize data to db format
            let mut buf = Vec::new();
            while let Ok(mut task) = rx.recv() {
                task(&mut buf);
            }
        });

        Self { tx }
    }

    pub fn execute<F>(&self, task: F)
    where
        F: 'static + FnMut(&mut Vec<u8>) + Send,
    {
        let _ = self.tx.send(Box::new(task));
    }
}

#[derive(Debug)]
pub struct UnstoreError;

#[derive(Debug)]
pub struct DataSized<T, const SIZE: usize> {
    kind: PhantomData<T>,
    pub data: [u8; SIZE],
}

impl<T, const SIZE: usize> DataSized<T, SIZE> {
    pub fn new(data: [u8; SIZE]) -> Self {
        DataSized {
            kind: PhantomData::<T>,
            data,
        }
    }
}

impl<T, const SIZE: usize> DataSized<T, SIZE>
where
    T: StoreSized<SIZE>,
{
    pub fn unstore_sized(self) -> Result<T, UnstoreError> {
        T::unstore_sized(self)
    }
}

pub trait StoreSized<const SIZE: usize> {
    fn store_sized(&self) -> DataSized<Self, SIZE>
    where
        Self: Sized;
    fn unstore_sized(stored: DataSized<Self, SIZE>) -> Result<Self, UnstoreError>
    where
        Self: Sized;
}

#[derive(Debug)]
pub enum DataContainer<'a> {
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

#[derive(Debug)]
pub struct Data<'a, T> {
    kind: PhantomData<T>,
    pub data: DataContainer<'a>,
}

const COMPRESS_LENGTH: usize = 100;

impl<'a, T> Data<'a, T> {
    pub fn new_shared(data: &'a [u8]) -> Self {
        Self {
            kind: PhantomData::<T>,
            data: DataContainer::Shared(data),
        }
    }

    pub fn new_owned(data: Vec<u8>) -> Self {
        Self {
            kind: PhantomData::<T>,
            data: DataContainer::Owned(data),
        }
    }
}

impl<'a, T> Data<'a, T>
where
    T: Store,
{
    pub fn unstore(self) -> Result<T, UnstoreError> {
        T::unstore(self)
    }
}

pub trait Store {
    fn store<'a>(&'_ self, buf: &'a mut Vec<u8>) -> Data<'a, Self>
    where
        Self: Sized;
    fn store_owned(&self) -> Data<Self>
    where
        Self: Sized;
    fn unstore(stored: Data<Self>) -> Result<Self, UnstoreError>
    where
        Self: Sized;
}

pub trait StoreDefault {}

// TODO fix if Write and Read gets implemented for the postcard
impl<T> Store for T
where
    T: Serialize + DeserializeOwned + StoreDefault,
{
    fn store<'a>(&'_ self, buf: &'a mut Vec<u8>) -> Data<'a, Self> {
        buf.clear();
        match postcard::to_slice(self, buf.as_mut_slice()) {
            Ok(_) => {},
            Err(Error::SerializeBufferFull) => {
                let mut new_buf = postcard::to_allocvec(self).unwrap();

                mem::swap(&mut new_buf, buf);
            },
            Err(err) => panic!("serialization error: {:?}", err),
        }

        if buf.len() > COMPRESS_LENGTH {
            // 1 is compression flag, 4 is uncompressed size
            let max_output_size = 5 + lz4::get_maximum_output_size(buf.len());
            let mut compressed = Vec::with_capacity(max_output_size.max(buf.capacity()));
            compressed.resize(max_output_size, 0);
            compressed[0] = 1;
            compressed[1 .. 5].copy_from_slice(&(buf.len() as u32).to_le_bytes());
            let len = lz4::compress_into(buf, &mut compressed[5 ..]).unwrap();
            compressed.truncate(5 + len);
            mem::swap(buf, &mut compressed);
        } else {
            buf.insert(0, 0);
        }

        Data::new_shared(buf)
    }

    fn store_owned(&self) -> Data<Self> {
        let mut buf = postcard::to_allocvec(&self).unwrap();

        if buf.len() > COMPRESS_LENGTH {
            // 1 is compression flag, 4 is uncompressed size
            let max_output_size = 5 + lz4::get_maximum_output_size(buf.len());
            let mut compressed = Vec::with_capacity(max_output_size.max(buf.capacity()));
            compressed.resize(max_output_size, 0);
            compressed[0] = 1;
            compressed[1 .. 5].copy_from_slice(&(buf.len() as u32).to_le_bytes());
            let len = lz4::compress_into(&buf, &mut compressed[5 ..]).unwrap();
            compressed.truncate(5 + len);
            mem::swap(&mut buf, &mut compressed);
        } else {
            buf.insert(0, 0);
        }

        Data::new_owned(buf)
    }

    fn unstore(stored: Data<T>) -> Result<Self, UnstoreError>
    where
        Self: Sized,
    {
        let buf = stored.data.as_ref();

        match buf.first() {
            Some(0) => Ok(postcard::from_bytes::<Self>(&buf[1 ..]).map_err(|_| UnstoreError)?),
            Some(1) => {
                let size = u32::from_le_bytes(buf[1 .. 5].try_into().unwrap());

                let decompressed =
                    lz4::decompress(&buf[5 ..], size as usize).map_err(|_| UnstoreError)?;

                Ok(postcard::from_bytes::<Self>(&decompressed).map_err(|_| UnstoreError)?)
            },
            _ => Err(UnstoreError),
        }
    }
}

pub mod player {
    use crate::storage::{
        Data,
        StoreDefault,
    };
    use redb::{
        RedbValue,
        TypeName,
    };
    use serde::{
        Deserialize,
        Serialize,
    };
    use serde_big_array::BigArray;

    #[derive(Serialize, Deserialize, Debug)]
    pub struct PlayerProfile {
        pub username: String,
        #[serde(with = "BigArray")]
        pub public_key: [u8; 33],
    }

    impl StoreDefault for PlayerProfile {}

    impl RedbValue for Data<'_, PlayerProfile> {
        type AsBytes<'a> = &'a [u8]
        where
            Self: 'a;
        type SelfType<'a> = Data<'a, PlayerProfile>
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

        fn type_name() -> TypeName {
            TypeName::new("PlayerProfile")
        }
    }
}
