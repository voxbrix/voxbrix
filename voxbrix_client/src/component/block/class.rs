use voxbrix_common::{
    component::block::{
        BlockComponentSimple,
        BlocksVec,
    },
    entity::block_class::BlockClass,
};

pub type ClassBlockComponent = BlockComponentSimple<BlocksVec<BlockClass>>;
