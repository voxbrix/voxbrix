use postcard::Error;
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::mem;

#[derive(Debug)]
pub struct UnpackError;

pub trait Pack {
    fn pack(&self, buf: &mut Vec<u8>);
    fn pack_to_vec(&self) -> Vec<u8>;
    fn unpack<R>(buf: R) -> Result<Self, UnpackError>
    where
        Self: Sized,
        R: AsRef<[u8]>;
}

pub trait PackDefault {}

impl<T> Pack for T
where
    T: Serialize + DeserializeOwned + PackDefault,
{
    fn pack(&self, buf: &mut Vec<u8>) {
        buf.clear();
        match postcard::to_slice(self, buf.as_mut_slice()) {
            Ok(_) => {},
            Err(Error::SerializeBufferFull) => {
                let mut new_buf = postcard::to_allocvec(self).unwrap();

                mem::swap(&mut new_buf, buf);
            },
            Err(err) => panic!("serialization error: {:?}", err),
        }
    }

    fn pack_to_vec(&self) -> Vec<u8> {
        postcard::to_allocvec(self).unwrap()
    }

    fn unpack<R>(buf: R) -> Result<Self, UnpackError>
    where
        Self: Sized,
        R: AsRef<[u8]>,
    {
        Ok(postcard::from_bytes::<Self>(buf.as_ref()).map_err(|_| UnpackError)?)
    }
}
