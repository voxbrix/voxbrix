use voxbrix_common::{
    component::block::{
        BlockComponentSimple,
        BlocksVec,
    },
    entity::block_environment::BlockEnvironment,
};

pub type EnvironmentBlockComponent = BlockComponentSimple<BlocksVec<BlockEnvironment>>;
