use crate::{
    component::StaticEntityComponent,
    entity::block_class::BlockClass,
};

pub mod collision;
pub mod opacity;

pub type BlockClassComponent<T> = StaticEntityComponent<BlockClass, T>;
