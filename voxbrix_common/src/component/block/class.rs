use crate::{
    component::block::{
        BlockComponent,
        BlocksVec,
    },
    entity::block_class::BlockClass,
};
pub type ClassBlockComponent = BlockComponent<BlocksVec<BlockClass>>;
