use crate::component::{
    actor::{
        player::PlayerActorComponent,
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
    },
    block::class::ClassBlockComponent,
};
use rayon::prelude::*;
use std::{
    mem,
    time::Duration,
};
use voxbrix_common::{
    component::{
        actor::{
            position::Position,
            velocity::Velocity,
        },
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::actor::Actor,
    system::position,
};

pub struct Change {
    pub actor: Actor,
    pub prev_position: Position,
    pub next_position: Position,
    pub prev_velocity: Velocity,
    pub next_velocity: Velocity,
    pub collides_with_block: bool,
}

pub struct PositionSystem {
    position_changes: Vec<Change>,
}

impl PositionSystem {
    pub fn new() -> Self {
        Self {
            position_changes: Vec::new(),
        }
    }

    pub fn collect_changes(
        &mut self,
        dt: Duration,
        class_bc: &ClassBlockComponent,
        collision_bcc: &CollisionBlockClassComponent,
        position_ac: &PositionActorComponent,
        velocity_ac: &VelocityActorComponent,
        player_ac: &PlayerActorComponent,
    ) {
        // TODO: replace
        let h_radius = 0.45;
        let v_radius = 0.95;
        let radius = [h_radius, h_radius, v_radius];

        self.position_changes.clear();

        let par_iter = velocity_ac
            .par_iter()
            .filter(|(actor, _)| player_ac.get(actor).is_none())
            .filter_map(|(actor, velocity)| {
                let position = position_ac.get(&actor)?;

                let mut collides_with_block = false;

                let (next_pos, next_vel) = position::process_actor(
                    dt,
                    class_bc,
                    collision_bcc,
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

        self.position_changes.par_extend(par_iter);
    }

    pub fn take_changes(&mut self) -> Vec<Change> {
        mem::take(&mut self.position_changes)
    }
}
