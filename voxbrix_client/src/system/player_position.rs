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
    resource::{
        player_actor::PlayerActor,
        player_actor_movement_metadata::PlayerActorMovementMetadata,
    },
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
    player_actor_mm: &'a mut PlayerActorMovementMetadata,
    velocity_ac: &'a mut VelocityActorComponent,
    block_collision_acc: &'a BlockCollisionActorClassComponent,
}

impl PlayerPositionSystemData<'_> {
    pub fn run(self) {
        if let Some((mut velocity, mut position)) = self
            .velocity_ac
            .get_writable(&self.player_actor.0, *self.snapshot)
            .zip(
                self.position_ac
                    .get_writable(&self.player_actor.0, *self.snapshot),
            )
        {
            let actor = self.player_actor.0;
            let Some(actor_class) = self.class_ac.get(&actor) else {
                return;
            };
            let radius = match self.block_collision_acc.get(actor_class, &actor) {
                BlockCollision::None => None,
                BlockCollision::AABB { radius_blocks } => Some(radius_blocks),
            };

            let position::ProcessActorResult {
                position: new_pos,
                collision_sides,
                velocity: new_vel,
            } = position::process_actor(
                self.process_timer.elapsed(),
                self.class_bc,
                self.collision_bcc,
                &position,
                &velocity,
                radius,
                |_, _| {},
                |_, _| {},
            );

            position.update(new_pos);
            velocity.update(new_vel);

            self.player_actor_mm.stands_on_surface = collision_sides[4];
        }
    }
}
