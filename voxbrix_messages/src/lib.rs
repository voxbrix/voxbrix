use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug)]
pub struct PackError;

#[derive(Debug)]
pub struct UnpackError;

pub mod client;
pub mod server;

#[derive(Serialize, Deserialize)]
pub struct Chunk {
    pub position: [i32; 3],
    pub dimension: u32,
}

pub trait Pack {
    fn pack(&self, buf: &mut Vec<u8>) -> Result<(), PackError>;
    fn unpack<R>(buf: R) -> Result<Self, UnpackError>
    where
        Self: Sized,
        R: AsRef<[u8]>;
}
