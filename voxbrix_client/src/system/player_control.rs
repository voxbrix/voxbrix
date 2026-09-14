use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            locomotion::LocomotionActorComponent,
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
        actor::{
            locomotion::Locomotion,
            orientation::Orientation,
            velocity::Velocity,
        },
        actor_class::propulsion::PropulsionType,
        block_environment::density::DensityBlockEnvironmentComponent,
    },
    entity::{
        block::Block,
        snapshot::ClientSnapshot,
    },
    math::{
        Directions,
        Vec3F32,
    },
    resource::tick_timer::TickTimer,
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
    tick_timer: &'a TickTimer,
    player_actor: &'a PlayerActor,
    player_movement: &'a mut PlayerInput,
    player_actor_mm: &'a PlayerActorMovementMetadata,
    class_ac: &'a ClassActorComponent,
    position_ac: &'a PositionActorComponent,
    propulsion_acc: &'a PropulsionActorClassComponent,
    environment_bc: &'a EnvironmentBlockComponent,
    density_bec: &'a DensityBlockEnvironmentComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    locomotion_ac: &'a mut LocomotionActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
}

impl PlayerControlSystemData<'_> {
    pub fn run(self) {
        let actor = self.player_actor.0;
        let snapshot = *self.snapshot;
        let dt = self.tick_timer.elapsed();

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
        let direction = if self.player_actor_mm.stands_on_surface {
            self.player_movement.surface_direction(Vec3F32::UP)
        } else {
            self.player_movement.direction()
        };
        let locomotion = direction
            .map(|direction| {
                Locomotion {
                    propulsion: if self.player_actor_mm.stands_on_surface {
                        PropulsionType::Ground
                    } else {
                        PropulsionType::Buoyant
                    },
                    direction,
                }
            })
            .unwrap_or_default();

        if let Some(mut actor_locomotion) = self.locomotion_ac.get_writable(&actor, snapshot) {
            actor_locomotion.update(locomotion);
        }

        if let Some(mut actor_velocity) = self.velocity_ac.get_writable(&actor, snapshot) {
            if self.player_actor_mm.stands_on_surface {
                let mut movement = surface_locomotion_to_world(locomotion.direction, orientation)
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

            let Some(direction) = locomotion_to_world(locomotion.direction, orientation) else {
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

fn locomotion_to_world(direction: Vec3F32, orientation: Orientation) -> Option<Vec3F32> {
    // Locomotion direction is stored in actor-local axes so it can be replicated
    // independently from the actor orientation: x = forward intent, y = right
    // intent, z = up/down intent.
    let mut direction = orientation.forward() * direction.x
        + orientation.right() * direction.y
        + orientation.up() * direction.z;

    if direction.is_nan() {
        return None;
    }

    // Normalize after combining axes so diagonal input does not move faster than
    // cardinal input.
    direction = direction.normalize();
    (!direction.is_nan()).then_some(direction)
}

fn surface_locomotion_to_world(direction: Vec3F32, orientation: Orientation) -> Option<Vec3F32> {
    // Ground locomotion uses the same local forward/right intent, but projects
    // those axes onto the current flat surface before applying movement speed.
    // This keeps walking speed parallel to the ground even when the camera is
    // pitched up or down. When wall/ceiling crawling is added, this should use
    // the resolved surface normal instead of assuming Vec3F32::UP.
    let mut forward = orientation.forward();
    let mut right = orientation.right();

    forward -= Vec3F32::UP * forward.dot(Vec3F32::UP);
    right -= Vec3F32::UP * right.dot(Vec3F32::UP);

    let forward = forward.normalize();
    let right = right.normalize();

    let mut direction = if forward.is_nan() || right.is_nan() {
        // Looking straight along the surface normal makes horizontal movement
        // undefined. Keep only local z movement in that degenerate case.
        Vec3F32::new(0.0, 0.0, direction.z)
    } else {
        forward * direction.x + right * direction.y + Vec3F32::UP * direction.z
    };

    if direction.is_nan() {
        return None;
    }

    // Normalize after combining axes so diagonal input does not move faster than
    // cardinal input.
    direction = direction.normalize();
    (!direction.is_nan()).then_some(direction)
}
