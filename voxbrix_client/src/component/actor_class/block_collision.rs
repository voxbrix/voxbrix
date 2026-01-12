use crate::component::actor_class::{
    OverridableActorClassComponent,
    OverridableFromDescriptor,
};
use voxbrix_common::component::actor_class::block_collision::BlockCollision;

pub type BlockCollisionActorClassComponent = OverridableActorClassComponent<BlockCollision>;

impl OverridableFromDescriptor for BlockCollision {
    const IS_CLIENT_CONTROLLED: bool = false;
    const UPDATE_LABEL: &str = "actor_block_collision";
}
