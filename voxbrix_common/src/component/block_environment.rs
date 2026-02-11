use crate::{
    component::StaticEntityComponent,
    entity::block_environment::BlockEnvironment,
};

pub type BlockEnvironmentComponent<T> = StaticEntityComponent<BlockEnvironment, T>;
