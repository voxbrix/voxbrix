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
            gravity_sensitivity::GravitySensitivityActorClassComponent,
        },
        block::environment::EnvironmentBlockComponent,
    },
    resource::{
        player_actor::PlayerActor,
        tick_timer::TickTimer,
    },
};
use voxbrix_common::{
    component::{
        actor::velocity::Velocity,
        block_environment::density::DensityBlockEnvironmentComponent,
        dimension_kind::gravity::{
            density_gravity_scale,
            GravityDimensionKindComponent,
        },
    },
    entity::{
        block::Block,
        snapshot::ClientSnapshot,
    },
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PlayerGravitySystem;

impl System for PlayerGravitySystem {
    type Data<'a> = PlayerGravitySystemData<'a>;
}

#[derive(SystemData)]
pub struct PlayerGravitySystemData<'a> {
    snapshot: &'a ClientSnapshot,
    tick_timer: &'a TickTimer,
    player_actor: &'a PlayerActor,
    position_ac: &'a PositionActorComponent,
    class_ac: &'a ClassActorComponent,
    gravity_sensitivity_acc: &'a GravitySensitivityActorClassComponent,
    density_acc: &'a DensityActorClassComponent,
    environment_bc: &'a EnvironmentBlockComponent,
    density_bec: &'a DensityBlockEnvironmentComponent,
    gravity_dkc: &'a GravityDimensionKindComponent,
    velocity_ac: &'a mut VelocityActorComponent,
}

impl PlayerGravitySystemData<'_> {
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
            let gravity_sensitivity = self.gravity_sensitivity_acc.get(actor_class, &actor).0;

            let env_density = Block::from_position(position.chunk, position.offset)
                .and_then(|(chunk, block)| {
                    let env = self.environment_bc.get_chunk(&chunk)?.get(block);
                    Some(self.density_bec.get(env))
                })
                .copied()
                .unwrap_or_default();

            let density_scale =
                density_gravity_scale(self.density_acc.get(actor_class, &actor).0, env_density.0);

            let dv = self
                .gravity_dkc
                .get(&position.chunk.dimension.kind)
                .into_velocity(self.tick_timer.elapsed());

            let new_velocity = *writable_velocity
                + Velocity {
                    vector: dv.vector * gravity_sensitivity * density_scale,
                };

            writable_velocity.update(new_velocity);
        }
    }
}
