use crate::storage::{
    DataSized,
    StoreSized,
    UnstoreError,
};
use redb::{
    RedbKey,
    RedbValue,
    TypeName,
};
use std::cmp::Ordering;
pub use voxbrix_common::entity::chunk::*;

pub const KEY_LENGTH: usize = 16;

impl RedbValue for DataSized<Chunk, KEY_LENGTH> {
    type AsBytes<'b> = &'b [u8; KEY_LENGTH]
    where
        Self: 'b;
    type SelfType<'b> = DataSized<Chunk, KEY_LENGTH>
    where
        Self: 'b;

    const ALIGNMENT: usize = 1usize;

    fn fixed_width() -> Option<usize> {
        Some(KEY_LENGTH)
    }

    fn from_bytes<'b>(data: &'b [u8]) -> Self::SelfType<'b>
    where
        Self: 'b,
    {
        DataSized::new(data.try_into().unwrap())
    }

    fn as_bytes<'b, 'c: 'b>(value: &'b Self::SelfType<'c>) -> Self::AsBytes<'b>
    where
        Self: 'b + 'c,
    {
        &value.data
    }

    fn type_name() -> TypeName {
        TypeName::new("Blocks<BlockClass>")
    }
}

impl RedbKey for DataSized<Chunk, KEY_LENGTH> {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        data1.cmp(data2)
    }
}

impl StoreSized<KEY_LENGTH> for Chunk {
    fn store_sized(&self) -> DataSized<Self, KEY_LENGTH> {
        let mut data = [0; KEY_LENGTH];
        let position = self.position.map(|x| u32_from_i32(x));

        data[0 .. 4].copy_from_slice(&self.dimension.to_be_bytes());
        data[4 .. 8].copy_from_slice(&position[2].to_be_bytes());
        data[8 .. 12].copy_from_slice(&position[1].to_be_bytes());
        data[12 .. 16].copy_from_slice(&position[0].to_be_bytes());

        DataSized::new(data)
    }

    fn unstore_sized(from: DataSized<Self, KEY_LENGTH>) -> Result<Self, UnstoreError> {
        let position = [
            u32::from_be_bytes(from.data[12 .. 16].try_into().unwrap()),
            u32::from_be_bytes(from.data[8 .. 12].try_into().unwrap()),
            u32::from_be_bytes(from.data[4 .. 8].try_into().unwrap()),
        ];

        Ok(Self {
            position: position.map(|x| i32_from_u32(x)).into(),
            dimension: u32::from_be_bytes(from.data[0 .. 4].try_into().unwrap()),
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
