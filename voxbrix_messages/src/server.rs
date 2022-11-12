use serde::{
    Serialize,
    Deserialize,
};

use std::mem;

use crate::{
    Pack,
    PackError,
    UnpackError,
    Chunk,
};

use postcard::Error;

#[derive(Serialize, Deserialize)]
pub enum ServerAccept {
    GetChunksBlocks {
        coords: Vec<Chunk>,
    },
}

impl Pack for ServerAccept {
    fn pack(&self, buf: &mut Vec<u8>) -> Result<(), PackError> {
        buf.clear();
        match postcard::to_slice(self, buf.as_mut_slice()) {
            Ok(_) => {},
            Err(Error::SerializeBufferFull) => {
                let mut new_buf = postcard::to_allocvec(self)
                    .map_err(|_| PackError)?;
                
                mem::swap(&mut new_buf, buf);
            },
            Err(_) => return Err(PackError),
        }

        Ok(())
    }

    fn unpack<R>(buf: R) -> Result<Self, UnpackError>
    where 
        Self: Sized,
        R: AsRef<[u8]>,
    {
        Ok(postcard::from_bytes::<Self>(buf.as_ref())
            .map_err(|_| UnpackError)?)
    }
}
