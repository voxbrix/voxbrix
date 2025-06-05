use crate::component::{
    actor::{
        player::PlayerActorComponent,
        position::{
            Change,
            PositionActorComponent,
            PositionChanges,
        },
        velocity::VelocityActorComponent,
    },
    block::class::ClassBlockComponent,
};
use rayon::prelude::*;
use voxbrix_common::{
    component::block_class::collision::CollisionBlockClassComponent,
    entity::snapshot::Snapshot,
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
    snapshot: &'a Snapshot,
    process_timer: &'a ProcessTimer,
    class_bc: &'a ClassBlockComponent,
    collision_bcc: &'a CollisionBlockClassComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    player_ac: &'a PlayerActorComponent,
    position_changes: &'a mut PositionChanges,
}

impl PositionSystemData<'_> {
    pub fn run(self) {
        let dt = self.process_timer.elapsed();
        // TODO: replace
        let h_radius = 0.45;
        let v_radius = 0.95;
        let radius = [h_radius, h_radius, v_radius];

        let par_iter = self
            .velocity_ac
            .par_iter()
            .filter(|(actor, _)| self.player_ac.get(actor).is_none())
            .filter_map(|(actor, velocity)| {
                let position = self.position_ac.get(&actor)?;

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

        for change in self.position_changes.iter() {
            self.position_ac
                .insert(change.actor, change.next_position, *self.snapshot);
            self.velocity_ac
                .insert(change.actor, change.next_velocity, *self.snapshot);
        }
    }
}
