use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
            WritableTrait,
        },
        actor_class::propulsion::PropulsionActorClassComponent,
        block::environment::EnvironmentBlockComponent,
    },
    resource::{
        player_actor::PlayerActor,
        player_actor_movement_metadata::PlayerActorMovementMetadata,
        player_input::PlayerInput,
    },
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

pub struct PlayerControlSystem;

impl System for PlayerControlSystem {
    type Data<'a> = PlayerControlSystemData<'a>;
}

#[derive(SystemData)]
pub struct PlayerControlSystemData<'a> {
    snapshot: &'a ClientSnapshot,
    process_timer: &'a ProcessTimer,
    player_actor: &'a PlayerActor,
    player_movement: &'a mut PlayerInput,
    player_actor_mm: &'a PlayerActorMovementMetadata,
    class_ac: &'a ClassActorComponent,
    position_ac: &'a PositionActorComponent,
    propulsion_acc: &'a PropulsionActorClassComponent,
    environment_bc: &'a EnvironmentBlockComponent,
    density_bec: &'a DensityBlockEnvironmentComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
}

impl PlayerControlSystemData<'_> {
    pub fn run(self) {
        let actor = self.player_actor.0;
        let snapshot = *self.snapshot;
        let dt = self.process_timer.elapsed();

        let Some(mut actor_orientation) = self.orientation_ac.get_writable(&actor, snapshot) else {
            return;
        };

        let mut orientation = *actor_orientation;

        self.player_movement
            .modify_orientation(dt, &mut orientation);

        actor_orientation.update(orientation);

        let Some(actor_class) = self.class_ac.get(&actor) else {
            return;
        };
        let propulsion = self.propulsion_acc.get(actor_class, &actor);
        let jump_requested = self.player_movement.take_jump_request();

        if let Some(mut actor_velocity) = self.velocity_ac.get_writable(&actor, snapshot) {
            if self.player_actor_mm.stands_on_surface {
                let mut movement = self
                    .player_movement
                    .horizontal_direction(orientation)
                    .map(|direction| direction * propulsion.ground.movement_speed)
                    .unwrap_or_default();

                if jump_requested {
                    movement[2] = propulsion.ground.jump_velocity;
                }

                actor_velocity.update(Velocity { vector: movement });

                return;
            }

            let env_density = self
                .position_ac
                .get(&actor)
                .and_then(|position| {
                    Block::from_position(position.chunk, position.offset).and_then(
                        |(chunk, block)| {
                            let env = self.environment_bc.get_chunk(&chunk)?.get(block);
                            Some(self.density_bec.get(env))
                        },
                    )
                })
                .copied()
                .unwrap_or_default();

            let Some(direction) = self.player_movement.direction(orientation) else {
                return;
            };

            let max_speed = propulsion.buoyant.max_speed.max(0.0);
            // Scalar projection: current speed along the intended propulsion direction.
            let velocity_projection = actor_velocity.vector.dot(direction);
            if velocity_projection >= max_speed {
                return;
            }

            let acceleration_delta = (propulsion.buoyant.acceleration(env_density.0)
                * dt.as_secs_f32())
            .min(max_speed - velocity_projection);

            if acceleration_delta <= 0.0 {
                return;
            }

            let movement = direction * acceleration_delta;

            actor_velocity.update(Velocity {
                vector: actor_velocity.vector + movement,
            });
        }
    }
}
