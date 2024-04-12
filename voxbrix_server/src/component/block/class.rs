use crate::component::block::TrackingBlockComponent;
use voxbrix_common::{
    component::block::BlocksVec,
    entity::block_class::BlockClass,
};

pub type ClassBlockComponent = TrackingBlockComponent<BlocksVec<BlockClass>>;
