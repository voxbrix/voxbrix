use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
            WritableTrait,
        },
        actor_class::block_collision::BlockCollisionActorClassComponent,
        block::class::ClassBlockComponent,
    },
    resource::player_actor::PlayerActor,
};
use voxbrix_common::{
    component::{
        actor_class::block_collision::BlockCollision,
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::snapshot::ClientSnapshot,
    resource::process_timer::ProcessTimer,
    system::position,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PlayerPositionSystem;

impl System for PlayerPositionSystem {
    type Data<'a> = PlayerPositionSystemData<'a>;
}

#[derive(SystemData)]
pub struct PlayerPositionSystemData<'a> {
    snapshot: &'a ClientSnapshot,
    process_timer: &'a ProcessTimer,
    player_actor: &'a PlayerActor,
    class_bc: &'a ClassBlockComponent,
    collision_bcc: &'a CollisionBlockClassComponent,
    class_ac: &'a ClassActorComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a VelocityActorComponent,
    block_collision_acc: &'a BlockCollisionActorClassComponent,
}

impl PlayerPositionSystemData<'_> {
    pub fn run(self) {
        if let Some((velocity, mut writable_position)) =
            self.velocity_ac.get(&self.player_actor.0).zip(
                self.position_ac
                    .get_writable(&self.player_actor.0, *self.snapshot),
            )
        {
            let actor = self.player_actor.0;
            let Some(actor_class) = self.class_ac.get(&actor) else {
                return;
            };
            let Some(block_collision) = self.block_collision_acc.get(&actor_class, &actor) else {
                return;
            };
            let radius = match block_collision {
                BlockCollision::AABB { radius_blocks } => radius_blocks,
            };

            let (new_pos, _new_vel) = position::process_actor(
                self.process_timer.elapsed(),
                self.class_bc,
                self.collision_bcc,
                &*writable_position,
                velocity,
                &radius,
                |_, _| {},
                |_, _| {},
            );

            writable_position.update(new_pos);
        }
    }
}
