use crate::component::{
    actor::{
        class::ClassActorComponent,
        player::PlayerActorComponent,
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
    },
    actor_class::drag::DragActorClassComponent,
    block::environment::EnvironmentBlockComponent,
};
use rayon::prelude::*;
use voxbrix_common::{
    component::{
        actor::velocity::Velocity,
        block_environment::density::DensityBlockEnvironmentComponent,
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

pub struct ActorDragSystem;

impl System for ActorDragSystem {
    type Data<'a> = ActorDragSystemData<'a>;
}

#[derive(SystemData)]
pub struct ActorDragSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    process_timer: &'a ProcessTimer,
    class_ac: &'a ClassActorComponent,
    player_ac: &'a PlayerActorComponent,
    position_ac: &'a PositionActorComponent,
    drag_acc: &'a DragActorClassComponent,
    environment_bc: &'a EnvironmentBlockComponent,
    density_bec: &'a DensityBlockEnvironmentComponent,
    velocity_ac: &'a mut VelocityActorComponent,
}

impl ActorDragSystemData<'_> {
    pub fn run(self) {
        let dt = self.process_timer.elapsed();
        let snapshot = *self.snapshot;

        let updates: Vec<(Actor, Velocity)> = self
            .velocity_ac
            .par_iter()
            .filter(|(actor, _)| self.player_ac.get(actor).is_none())
            .filter_map(|(actor, velocity)| {
                let position = self.position_ac.get(&actor)?;
                let actor_class = self.class_ac.get(&actor)?;

                let env_density = Block::from_position(position.chunk, position.offset)
                    .and_then(|(chunk, block)| {
                        let env = self.environment_bc.get_chunk(&chunk)?.get(block);
                        Some(self.density_bec.get(env))
                    })
                    .copied()
                    .unwrap_or_default();

                let new_velocity = Velocity {
                    vector: self.drag_acc.get(actor_class, &actor).apply(
                        velocity.vector,
                        env_density.0,
                        dt,
                    ),
                };

                (new_velocity != *velocity).then_some((actor, new_velocity))
            })
            .collect();

        for (actor, new_velocity) in updates {
            self.velocity_ac.insert(actor, new_velocity, snapshot);
        }
    }
}
