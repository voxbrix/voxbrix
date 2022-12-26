use crate::storage::AsKey;
pub use voxbrix_common::entity::block::*;

// 4b dimension, 12b chunk coords, 2b block coords (u16)
pub const KEY_LENGTH: usize = 18;

impl AsKey<KEY_LENGTH> for Block {
    fn write_key(&self, buf: &mut [u8]) {
        buf[KEY_LENGTH - 2 .. KEY_LENGTH].copy_from_slice(&(self.0 as u16).to_be_bytes());
    }

    fn read_key<B>(buf: B) -> Self
    where
        Self: Sized,
        B: AsRef<[u8]>,
    {
        let buf: &[u8] = buf.as_ref();

        let idx = u16::from_be_bytes(buf[KEY_LENGTH - 2 .. KEY_LENGTH].try_into().unwrap());

        Self(idx as usize)
    }

    fn to_key(self) -> [u8; KEY_LENGTH] {
        let mut buf = [0; KEY_LENGTH];
        buf[KEY_LENGTH - 2 .. KEY_LENGTH].copy_from_slice(&(self.0 as u16).to_be_bytes());

        buf
    }

    fn from_key(from: [u8; KEY_LENGTH]) -> Self {
        let idx = u16::from_be_bytes(from[KEY_LENGTH - 2 .. KEY_LENGTH].try_into().unwrap());

        Self(idx as usize)
    }
}
