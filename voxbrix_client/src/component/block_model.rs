use crate::entity::block_model::BlockModel;
use voxbrix_common::component::StaticEntityComponent;

pub mod builder;
pub mod culling;

pub type BlockModelComponent<T> = StaticEntityComponent<BlockModel, T>;
