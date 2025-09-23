use crate::component::{
    actor::{
        class::ClassActorComponent,
        player::PlayerActorComponent,
        position::{
            Change,
            PositionActorComponent,
            PositionChanges,
        },
        velocity::VelocityActorComponent,
    },
    actor_class::block_collision::BlockCollisionActorClassComponent,
    block::class::ClassBlockComponent,
};
use rayon::prelude::*;
use voxbrix_common::{
    component::{
        actor_class::block_collision::BlockCollision,
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::snapshot::ServerSnapshot,
    resource::process_timer::ProcessTimer,
    system::position,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PositionSystem;

impl System for PositionSystem {
    type Data<'a> = PositionSystemData<'a>;
}

#[derive(SystemData)]
pub struct PositionSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    process_timer: &'a ProcessTimer,
    class_bc: &'a ClassBlockComponent,
    collision_bcc: &'a CollisionBlockClassComponent,
    class_ac: &'a ClassActorComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    player_ac: &'a PlayerActorComponent,
    block_collision_acc: &'a BlockCollisionActorClassComponent,
    position_changes: &'a mut PositionChanges,
}

impl PositionSystemData<'_> {
    pub fn run(self) {
        let dt = self.process_timer.elapsed();

        let par_iter = self.velocity_ac.par_iter().filter_map(|(actor, velocity)| {
            let position = self.position_ac.get(&actor)?;
            let actor_class = self.class_ac.get(&actor)?;
            let radius = match self.block_collision_acc.get(&actor_class, &actor)? {
                BlockCollision::AABB { radius_blocks } => radius_blocks,
            };

            let mut collides_with_block = false;

            let (next_pos, next_vel) = position::process_actor(
                dt,
                self.class_bc,
                self.collision_bcc,
                &position,
                velocity,
                &radius,
                |_, _| {},
                |_, _| {
                    collides_with_block = true;
                },
            );

            // TODO only add if the actor has collision component
            // AND insert any static (no velocity component) actors
            //     that have collision component before using
            Some(Change {
                actor,
                prev_position: *position,
                next_position: next_pos,
                prev_velocity: *velocity,
                next_velocity: next_vel,
                collides_with_block,
            })
        });

        self.position_changes.from_par_iter(par_iter);

        for change in self
            .position_changes
            .iter()
            .filter(|change| self.player_ac.get(&change.actor).is_none())
        {
            self.position_ac
                .insert(change.actor, change.next_position, *self.snapshot);
            self.velocity_ac
                .insert(change.actor, change.next_velocity, *self.snapshot);
        }
    }
}
