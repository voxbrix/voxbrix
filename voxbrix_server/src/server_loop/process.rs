use crate::system::{
    actor_block_collision::ActorBlockCollisionSystem,
    actor_pruning::ActorPruningSystem,
    actor_sync::ActorSyncSystem,
    block_sync::BlockSyncSystem,
    chunk_activation::ChunkActivationSystem,
    chunk_sending::ChunkSendingSystem,
    position::PositionSystem,
};
use voxbrix_common::{
    entity::snapshot::ServerSnapshot,
    resource::process_timer::ProcessTimer,
};
use voxbrix_world::World;

pub struct Process<'a> {
    pub world: &'a mut World,
}

impl Process<'_> {
    pub fn run(self) {
        let Self { world } = self;

        world.get_resource_mut::<ProcessTimer>().record_next();

        world.get_data::<ChunkSendingSystem>().run();

        world.get_data::<BlockSyncSystem>().run();

        world.get_data::<PositionSystem>().run();

        world.get_data::<ActorBlockCollisionSystem>().run();

        world.get_data::<ActorSyncSystem>().run();

        world.get_data::<ChunkActivationSystem>().run();

        world.get_data::<ActorPruningSystem>().run();

        let snapshot = world.get_resource_mut::<ServerSnapshot>();

        *snapshot = snapshot.next();
    }
}
