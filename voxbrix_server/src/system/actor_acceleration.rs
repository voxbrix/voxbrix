use crate::component::{
    actor::{
        class::ClassActorComponent,
        player::PlayerActorComponent,
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
    },
    actor_class::{
        density::DensityActorClassComponent,
        dimension_acceleration::DimensionAccelerationActorClassComponent,
    },
    block::environment::EnvironmentBlockComponent,
};
use rayon::prelude::*;
use voxbrix_common::{
    component::{
        actor::velocity::Velocity,
        block_environment::density::DensityBlockEnvironmentComponent,
        dimension_kind::acceleration::{
            density_acceleration_scale,
            AccelerationDimensionKindComponent,
        },
    },
    entity::{
        actor::Actor,
        block::Block,
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
    density_acc: &'a DensityActorClassComponent,
    environment_bc: &'a EnvironmentBlockComponent,
    density_bec: &'a DensityBlockEnvironmentComponent,
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

                let dim_acc_scalar = self.dimension_acceleration_acc.get(actor_class, &actor).0;

                let env_density = Block::from_position(position.chunk, position.offset)
                    .and_then(|(chunk, block)| {
                        let env = self.environment_bc.get_chunk(&chunk)?.get(block);
                        Some(self.density_bec.get(env))
                    })
                    .copied()
                    .unwrap_or_default();

                let density_scale = density_acceleration_scale(
                    self.density_acc.get(actor_class, &actor).0,
                    env_density.0,
                );

                let dv = self
                    .acceleration_dkc
                    .get(&position.chunk.dimension.kind)
                    .into_velocity(dt);

                let new_velocity = *velocity
                    + Velocity {
                        vector: dv.vector * dim_acc_scalar * density_scale,
                    };

                (new_velocity != *velocity).then_some((actor, new_velocity))
            })
            .collect();

        for (actor, new_velocity) in updates {
            self.velocity_ac.insert(actor, new_velocity, snapshot);
        }
    }
}
