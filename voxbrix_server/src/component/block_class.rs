use crate::{
    component::block::Blocks,
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

impl StoreDefault for Blocks<BlockClass> {}

impl RedbValue for Data<'_, Blocks<BlockClass>> {
    type AsBytes<'a> = &'a [u8]
    where
        Self: 'a;
    type SelfType<'a> = Data<'a, Blocks<BlockClass>>
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
        Data::new(data)
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a + 'b,
    {
        value.data
    }

    fn type_name() -> TypeName {
        TypeName::new("Blocks<BlockClass>")
    }
}
