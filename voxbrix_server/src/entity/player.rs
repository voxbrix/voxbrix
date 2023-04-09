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
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    cmp::Ordering,
    mem,
};
use voxbrix_common::pack::PackDefault;

pub const KEY_LENGTH: usize = mem::size_of::<usize>();

#[derive(Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq, Clone, Copy, Debug)]
pub struct Player(pub usize);

impl PackDefault for Player {}

impl StoreSized<KEY_LENGTH> for Player {
    fn store_sized(&self) -> DataSized<Self, KEY_LENGTH> {
        DataSized::new(self.0.to_be_bytes())
    }

    fn unstore_sized(stored: DataSized<Self, KEY_LENGTH>) -> Result<Self, UnstoreError> {
        Ok(Self(usize::from_be_bytes(stored.data)))
    }
}

impl RedbValue for DataSized<Player, KEY_LENGTH> {
    type AsBytes<'a> = &'a [u8]
    where
        Self: 'a;
    type SelfType<'a> = DataSized<Player, KEY_LENGTH>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        DataSized::new(data.try_into().unwrap())
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a + 'b,
    {
        &value.data
    }

    fn type_name() -> TypeName {
        TypeName::new("Player")
    }
}

impl RedbKey for DataSized<Player, KEY_LENGTH> {
    fn compare(data1: &[u8], data2: &[u8]) -> Ordering {
        data1.cmp(data2)
    }
}
