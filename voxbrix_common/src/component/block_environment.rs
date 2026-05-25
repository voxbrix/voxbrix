use crate::{
    component::StaticEntityComponent,
    entity::block_environment::BlockEnvironment,
};

pub mod density;

pub type BlockEnvironmentComponent<T> = StaticEntityComponent<BlockEnvironment, T>;
