use crate::system::{
    actor_pruning::ActorPruningSystem,
    actor_sync::ActorSyncSystem,
    block_sync::BlockSyncSystem,
    chunk_activation::ChunkActivationSystem,
    chunk_sending::ChunkSendingSystem,
    effect_snapshot::EffectSnapshotSystem,
    position::PositionSystem,
    projectile_actor_handling::ProjectileActorHandlingSystem,
    projectile_block_handling::ProjectileBlockHandlingSystem,
    projectile_hitbox_collision::ProjectileHitboxCollisionSystem,
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

        world.get_data::<ProjectileHitboxCollisionSystem>().run();

        world.get_data::<ProjectileActorHandlingSystem>().run();

        world.get_data::<ProjectileBlockHandlingSystem>().run();

        world.get_data::<EffectSnapshotSystem>().run();

        world.get_data::<ActorSyncSystem>().run();

        world.get_data::<ChunkActivationSystem>().run();

        world.get_data::<ActorPruningSystem>().run();

        let snapshot = world.get_resource_mut::<ServerSnapshot>();

        *snapshot = snapshot.next();
    }
}
