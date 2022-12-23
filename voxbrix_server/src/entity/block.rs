use crate::store::AsKey;
pub use voxbrix_common::entity::block::*;

// 4b dimension, 12b chunk coords, 2b block coords (u16)
pub const KEY_LENGTH: usize = 18;

impl AsKey for Block {
    fn to_key(&self, buf: &mut [u8]) {
        buf[KEY_LENGTH - 2 .. KEY_LENGTH].copy_from_slice(&(self.0 as u16).to_be_bytes());
    }

    fn from_key<B>(buf: B) -> Self
    where
        Self: Sized,
        B: AsRef<[u8]>,
    {
        let buf: &[u8] = buf.as_ref();

        let idx = u16::from_be_bytes(buf[KEY_LENGTH - 2 .. KEY_LENGTH].try_into().unwrap());

        Self(idx as usize)
    }
}
