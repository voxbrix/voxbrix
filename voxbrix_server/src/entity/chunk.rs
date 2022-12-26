use crate::storage::AsKey;
pub use voxbrix_common::entity::chunk::*;

impl AsKey<16> for Chunk {
    fn write_key(&self, buf: &mut [u8]) {
        let position = self.position.map(|x| u32_from_i32(x));

        buf[0 .. 4].copy_from_slice(&self.dimension.to_be_bytes());
        buf[4 .. 8].copy_from_slice(&position[2].to_be_bytes());
        buf[8 .. 12].copy_from_slice(&position[1].to_be_bytes());
        buf[12 .. 16].copy_from_slice(&position[0].to_be_bytes());
    }

    fn read_key<B>(buf: B) -> Self
    where
        Self: Sized,
        B: AsRef<[u8]>,
    {
        let buf: &[u8] = buf.as_ref();

        let position = [
            u32::from_be_bytes(buf[12 .. 16].try_into().unwrap()),
            u32::from_be_bytes(buf[8 .. 12].try_into().unwrap()),
            u32::from_be_bytes(buf[4 .. 8].try_into().unwrap()),
        ];

        Self {
            position: position.map(|x| i32_from_u32(x)).into(),
            dimension: u32::from_be_bytes(buf[0 .. 4].try_into().unwrap()),
        }
    }

    fn to_key(self) -> [u8; Self::KEY_LENGTH] {
        let mut buf = [0; Self::KEY_LENGTH];
        let position = self.position.map(|x| u32_from_i32(x));

        buf[0 .. 4].copy_from_slice(&self.dimension.to_be_bytes());
        buf[4 .. 8].copy_from_slice(&position[2].to_be_bytes());
        buf[8 .. 12].copy_from_slice(&position[1].to_be_bytes());
        buf[12 .. 16].copy_from_slice(&position[0].to_be_bytes());

        buf
    }

    fn from_key(from: [u8; Self::KEY_LENGTH]) -> Self {
        let position = [
            u32::from_be_bytes(from[12 .. 16].try_into().unwrap()),
            u32::from_be_bytes(from[8 .. 12].try_into().unwrap()),
            u32::from_be_bytes(from[4 .. 8].try_into().unwrap()),
        ];

        Self {
            position: position.map(|x| i32_from_u32(x)).into(),
            dimension: u32::from_be_bytes(from[0 .. 4].try_into().unwrap()),
        }
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
