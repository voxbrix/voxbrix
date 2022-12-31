use lz4_flex::block as lz4;
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

const COMPRESS_LENGTH: usize = 100;

pub trait PackZip {
    fn pack(&self, buf: &mut Vec<u8>);
    fn pack_to_vec(&self) -> Vec<u8>;
    fn unpack<R>(buf: R) -> Result<Self, UnpackError>
    where
        Self: Sized,
        R: AsRef<[u8]>;
}

pub trait PackZipDefault {}

// TODO fix if Write and Read gets implemented for the postcard
impl<T> PackZip for T
where
    T: Serialize + DeserializeOwned + PackZipDefault,
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

        if buf.len() > COMPRESS_LENGTH {
            // 1 is compression flag, 4 is uncompressed size
            let max_output_size = 5 + lz4::get_maximum_output_size(buf.len());
            let mut compressed = Vec::with_capacity(max_output_size.max(buf.capacity()));
            compressed.resize(max_output_size, 0);
            compressed[0] = 1;
            compressed[1 .. 5].copy_from_slice(&(buf.len() as u32).to_le_bytes());
            let len = lz4::compress_into(&buf, &mut compressed[5 ..]).unwrap();
            compressed.truncate(5 + len);
            mem::swap(buf, &mut compressed);
        } else {
            buf.insert(0, 0);
        }
    }

    fn pack_to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.pack(&mut buf);
        buf
    }

    fn unpack<R>(buf: R) -> Result<Self, UnpackError>
    where
        Self: Sized,
        R: AsRef<[u8]>,
    {
        let buf = buf.as_ref();

        match buf.get(0) {
            Some(0) => Ok(postcard::from_bytes::<Self>(&buf[1 ..]).map_err(|_| UnpackError)?),
            Some(1) => {
                let size = u32::from_le_bytes(buf[1 .. 5].try_into().unwrap());

                let compressed =
                    lz4::decompress(&buf[5 ..], size as usize).map_err(|_| UnpackError)?;

                Ok(postcard::from_bytes::<Self>(&compressed).map_err(|_| UnpackError)?)
            },
            _ => Err(UnpackError),
        }
    }
}
