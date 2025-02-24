use crate::storage::{
    IntoDataSized,
    TypeName,
};
use std::mem;
use voxbrix_common::entity::chunk::{
    Chunk,
    Dimension,
};

impl TypeName for Chunk {
    const NAME: &'static str = "Chunk";
}

impl IntoDataSized for Chunk {
    type Array = [u8; mem::size_of::<Self>()];

    fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut data = [0; Self::SIZE];
        let position = self.position.map(u32_from_i32);

        const DIM_LEN: usize = const { Dimension::BYTES_LEN };

        data[0 .. DIM_LEN].copy_from_slice(&self.dimension.to_be_bytes());
        data[DIM_LEN .. const { DIM_LEN + 4 }].copy_from_slice(&position[2].to_be_bytes());
        data[const { DIM_LEN + 4 } .. const { DIM_LEN + 8 }]
            .copy_from_slice(&position[1].to_be_bytes());
        data[const { DIM_LEN + 8 } .. const { DIM_LEN + 12 }]
            .copy_from_slice(&position[0].to_be_bytes());

        data
    }

    fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        const DIM_LEN: usize = const { Dimension::BYTES_LEN };

        let position = [
            u32::from_be_bytes(
                bytes[const { DIM_LEN + 8 } .. const { DIM_LEN + 12 }]
                    .try_into()
                    .unwrap(),
            ),
            u32::from_be_bytes(
                bytes[const { DIM_LEN + 4 } .. const { DIM_LEN + 8 }]
                    .try_into()
                    .unwrap(),
            ),
            u32::from_be_bytes(bytes[DIM_LEN .. const { DIM_LEN + 4 }].try_into().unwrap()),
        ];

        Self {
            position: position.map(i32_from_u32).into(),
            dimension: Dimension::from_be_bytes(bytes[0 .. DIM_LEN].try_into().unwrap()),
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
