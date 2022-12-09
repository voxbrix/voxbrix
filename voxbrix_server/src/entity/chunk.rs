use crate::store::{
    AsKey,
    FromKeyError,
    ToKeyError,
};
pub const KEY_LENGTH: usize = 16;

pub use voxbrix_common::entity::chunk::*;

impl AsKey for Chunk {
    fn to_key(&self, buf: &mut [u8]) -> Result<(), ToKeyError> {
        let position = self.position.map(|x| u32_from_i32(x));

        if buf.len() < KEY_LENGTH {
            return Err(ToKeyError::BufferTooSmall);
        }

        buf[0 .. 4].copy_from_slice(&self.dimension.to_be_bytes());
        buf[4 .. 8].copy_from_slice(&position[2].to_be_bytes());
        buf[8 .. 12].copy_from_slice(&position[1].to_be_bytes());
        buf[12 .. 16].copy_from_slice(&position[0].to_be_bytes());

        Ok(())
    }

    fn from_key<B>(buf: B) -> Result<Self, FromKeyError>
    where
        Self: Sized,
        B: AsRef<[u8]>,
    {
        let buf: &[u8] = buf.as_ref();

        if buf.len() != KEY_LENGTH {
            return Err(FromKeyError::IncorrectBufferSize);
        }

        let position = [
            u32::from_be_bytes(buf[12 .. 16].try_into().unwrap()),
            u32::from_be_bytes(buf[8 .. 12].try_into().unwrap()),
            u32::from_be_bytes(buf[4 .. 8].try_into().unwrap()),
        ];

        Ok(Self {
            position: position.map(|x| i32_from_u32(x)).into(),
            dimension: u32::from_be_bytes(buf[0 .. 4].try_into().unwrap()),
        })
    }
}

const I32_MIN_ABS: u32 = i32::MIN.abs_diff(0);

fn i32_from_u32(uint: u32) -> i32 {
    if uint >= I32_MIN_ABS {
        (uint - I32_MIN_ABS) as i32
    } else {
        i32::MIN + uint as i32
    }
}

fn u32_from_i32(int: i32) -> u32 {
    int.abs_diff(i32::MIN)
}
