use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
            WritableTrait,
        },
        actor_class::{
            density::DensityActorClassComponent,
            dimension_acceleration::DimensionAccelerationActorClassComponent,
        },
        block::environment::EnvironmentBlockComponent,
    },
    resource::player_actor::PlayerActor,
};
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
        block::Block,
        snapshot::ClientSnapshot,
    },
    resource::process_timer::ProcessTimer,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PlayerAccelerationSystem;

impl System for PlayerAccelerationSystem {
    type Data<'a> = PlayerAccelerationSystemData<'a>;
}

#[derive(SystemData)]
pub struct PlayerAccelerationSystemData<'a> {
    snapshot: &'a ClientSnapshot,
    process_timer: &'a ProcessTimer,
    player_actor: &'a PlayerActor,
    position_ac: &'a PositionActorComponent,
    class_ac: &'a ClassActorComponent,
    dimension_acceleration_acc: &'a DimensionAccelerationActorClassComponent,
    density_acc: &'a DensityActorClassComponent,
    environment_bc: &'a EnvironmentBlockComponent,
    density_bec: &'a DensityBlockEnvironmentComponent,
    acceleration_dkc: &'a AccelerationDimensionKindComponent,
    velocity_ac: &'a mut VelocityActorComponent,
}

impl PlayerAccelerationSystemData<'_> {
    pub fn run(self) {
        let actor = self.player_actor.0;

        let Some(actor_class) = self.class_ac.get(&actor) else {
            return;
        };

        if let Some((mut writable_velocity, position)) = self
            .velocity_ac
            .get_writable(&actor, *self.snapshot)
            .zip(self.position_ac.get(&actor))
        {
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
                .into_velocity(self.process_timer.elapsed());

            let new_velocity = *writable_velocity
                + Velocity {
                    vector: dv.vector * dim_acc_scalar * density_scale,
                };

            writable_velocity.update(new_velocity);
        }
    }
}
