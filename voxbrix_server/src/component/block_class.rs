use crate::{
    component::block::BlocksVec,
    entity::block_class::BlockClass,
    storage::{
        Data,
        StoreDefault,
    },
};
use redb::{
    RedbValue,
    TypeName,
};
pub use voxbrix_common::component::block_class::*;

impl StoreDefault for BlocksVec<BlockClass> {}

impl RedbValue for Data<'_, BlocksVec<BlockClass>> {
    type AsBytes<'a> = &'a [u8]
    where
        Self: 'a;
    type SelfType<'a> = Data<'a, BlocksVec<BlockClass>>
    where
        Self: 'a;

    const ALIGNMENT: usize = 1usize;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        Data::new_shared(data)
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a + 'b,
    {
        value.data.as_ref()
    }

    fn type_name() -> TypeName {
        TypeName::new("BlocksVec<BlockClass>")
    }
}
