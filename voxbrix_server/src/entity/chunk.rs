use crate::storage::{
    IntoDataSized,
    TypeName,
};
use voxbrix_common::entity::chunk::Chunk;

impl TypeName for Chunk {
    const NAME: &'static str = "Chunk";
}

impl IntoDataSized for Chunk {
    type Array = [u8; 16];

    fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut data = [0; Self::SIZE];
        let position = self.position.to_array().map(u32_from_i32);

        data[0 .. 4].copy_from_slice(&self.dimension.to_be_bytes());
        data[4 .. 8].copy_from_slice(&position[2].to_be_bytes());
        data[8 .. 12].copy_from_slice(&position[1].to_be_bytes());
        data[12 .. 16].copy_from_slice(&position[0].to_be_bytes());

        data
    }

    fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        let position = [
            u32::from_be_bytes(bytes[12 .. 16].try_into().unwrap()),
            u32::from_be_bytes(bytes[8 .. 12].try_into().unwrap()),
            u32::from_be_bytes(bytes[4 .. 8].try_into().unwrap()),
        ];

        Self {
            position: position.map(i32_from_u32).into(),
            dimension: u32::from_be_bytes(bytes[0 .. 4].try_into().unwrap()),
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
