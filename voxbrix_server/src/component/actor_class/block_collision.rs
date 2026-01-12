use crate::component::actor_class::{
    PackableOverridableActorClassComponent,
    WithUpdate,
};
use voxbrix_common::component::actor_class::block_collision::BlockCollision;

impl WithUpdate for BlockCollision {
    const UPDATE: &str = "actor_block_collision";
}

pub type BlockCollisionActorClassComponent = PackableOverridableActorClassComponent<BlockCollision>;
