use crate::storage::TypeName;
use voxbrix_common::{
    component::block::BlocksVec,
    entity::block_class::BlockClass,
};

impl TypeName for BlocksVec<BlockClass> {
    const NAME: &'static str = "BlocksVec<BlockClass>";
}
