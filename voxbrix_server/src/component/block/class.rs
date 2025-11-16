use crate::{
    component::block::TrackingBlockComponent,
    storage::TypeName,
};
use voxbrix_common::{
    component::block::BlocksVec,
    entity::block_class::BlockClass,
};

pub type ClassBlockComponent = TrackingBlockComponent<BlocksVec<BlockClass>>;

impl TypeName for BlocksVec<BlockClass> {
    const NAME: &'static str = "BlocksVec<BlockClass>";
}
