use crate::component::{
    actor::{
        class::ClassActorComponent,
        player::PlayerActorComponent,
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
    },
    actor_class::dimension_acceleration::DimensionAccelerationActorClassComponent,
};
use rayon::prelude::*;
use voxbrix_common::{
    component::{
        actor::velocity::Velocity,
        dimension_kind::acceleration::AccelerationDimensionKindComponent,
    },
    entity::{
        actor::Actor,
        snapshot::ServerSnapshot,
    },
    resource::process_timer::ProcessTimer,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ActorAccelerationSystem;

impl System for ActorAccelerationSystem {
    type Data<'a> = ActorAccelerationSystemData<'a>;
}

#[derive(SystemData)]
pub struct ActorAccelerationSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    process_timer: &'a ProcessTimer,
    class_ac: &'a ClassActorComponent,
    player_ac: &'a PlayerActorComponent,
    position_ac: &'a PositionActorComponent,
    dimension_acceleration_acc: &'a DimensionAccelerationActorClassComponent,
    acceleration_dkc: &'a AccelerationDimensionKindComponent,
    velocity_ac: &'a mut VelocityActorComponent,
}

impl ActorAccelerationSystemData<'_> {
    pub fn run(self) {
        let dt = self.process_timer.elapsed();
        let snapshot = *self.snapshot;

        // Compute new velocities, skipping no-op updates so the sequential
        // `insert` pass below avoids redundant per-snapshot bookkeeping.
        let updates: Vec<(Actor, Velocity)> = self
            .velocity_ac
            .par_iter()
            .filter(|(actor, _)| self.player_ac.get(actor).is_none())
            .filter_map(|(actor, velocity)| {
                let position = self.position_ac.get(&actor)?;
                let actor_class = self.class_ac.get(&actor)?;

                let scalar = self.dimension_acceleration_acc.get(actor_class, &actor).0;

                let dv = self
                    .acceleration_dkc
                    .get(&position.chunk.dimension.kind)
                    .into_velocity(dt);

                let new_velocity = *velocity
                    + Velocity {
                        vector: dv.vector * scalar,
                    };

                (new_velocity != *velocity).then_some((actor, new_velocity))
            })
            .collect();

        for (actor, new_velocity) in updates {
            self.velocity_ac.insert(actor, new_velocity, snapshot);
        }
    }
}
