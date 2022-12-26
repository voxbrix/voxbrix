use crate::storage::AsKey;
use serde::{
    Deserialize,
    Serialize,
};
use std::mem;
use voxbrix_common::pack::PackDefault;

pub const KEY_LENGTH: usize = mem::size_of::<u64>();

#[derive(Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq, Clone, Copy, Debug)]
pub struct Player(pub u64);

impl AsKey<KEY_LENGTH> for Player {
    fn write_key(&self, buf: &mut [u8]) {
        buf.copy_from_slice(&(self.0).to_be_bytes());
    }

    fn read_key<B>(buf: B) -> Self
    where
        Self: Sized,
        B: AsRef<[u8]>,
    {
        let buf: &[u8] = buf.as_ref();

        Self(u64::from_be_bytes(buf.try_into().unwrap()))
    }

    fn to_key(self) -> [u8; KEY_LENGTH] {
        self.0.to_be_bytes()
    }

    fn from_key(from: [u8; KEY_LENGTH]) -> Self {
        Player(u64::from_be_bytes(from))
    }
}

impl PackDefault for Player {}
