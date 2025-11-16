use crate::{
    component::block::TrackingBlockComponent,
    storage::TypeName,
};
use voxbrix_common::{
    component::block::BlocksVec,
    entity::block_environment::BlockEnvironment,
};

pub type EnvironmentBlockComponent = TrackingBlockComponent<BlocksVec<BlockEnvironment>>;

impl TypeName for BlocksVec<BlockEnvironment> {
    const NAME: &'static str = "BlocksVec<BlockEnvironment>";
}
