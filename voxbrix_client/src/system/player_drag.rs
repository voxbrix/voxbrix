use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
            WritableTrait,
        },
        actor_class::drag::DragActorClassComponent,
        block::environment::EnvironmentBlockComponent,
    },
    resource::player_actor::PlayerActor,
};
use voxbrix_common::{
    component::{
        actor::velocity::Velocity,
        block_environment::density::DensityBlockEnvironmentComponent,
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

pub struct PlayerDragSystem;

impl System for PlayerDragSystem {
    type Data<'a> = PlayerDragSystemData<'a>;
}

#[derive(SystemData)]
pub struct PlayerDragSystemData<'a> {
    snapshot: &'a ClientSnapshot,
    process_timer: &'a ProcessTimer,
    player_actor: &'a PlayerActor,
    position_ac: &'a PositionActorComponent,
    class_ac: &'a ClassActorComponent,
    drag_acc: &'a DragActorClassComponent,
    environment_bc: &'a EnvironmentBlockComponent,
    density_bec: &'a DensityBlockEnvironmentComponent,
    velocity_ac: &'a mut VelocityActorComponent,
}

impl PlayerDragSystemData<'_> {
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
            let env_density = Block::from_position(position.chunk, position.offset)
                .and_then(|(chunk, block)| {
                    let env = self.environment_bc.get_chunk(&chunk)?.get(block);
                    Some(self.density_bec.get(env))
                })
                .copied()
                .unwrap_or_default();

            let new_velocity = Velocity {
                vector: self.drag_acc.get(actor_class, &actor).apply(
                    writable_velocity.vector,
                    env_density.0,
                    self.process_timer.elapsed(),
                ),
            };

            writable_velocity.update(new_velocity);
        }
    }
}
