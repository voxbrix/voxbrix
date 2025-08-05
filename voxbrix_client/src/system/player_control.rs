use crate::{
    component::actor::{
        orientation::OrientationActorComponent,
        velocity::VelocityActorComponent,
        WritableTrait,
    },
    resource::{
        player_actor::PlayerActor,
        player_input::PlayerInput,
    },
};
use voxbrix_common::{
    component::actor::velocity::Velocity,
    entity::snapshot::ClientSnapshot,
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
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
}

impl PlayerControlSystemData<'_> {
    pub fn run(self) {
        let actor = self.player_actor.0;
        let snapshot = *self.snapshot;

        let mut actor_orientation = self.orientation_ac.get_writable(&actor, snapshot).unwrap();
        let mut actor_velocity = self.velocity_ac.get_writable(&actor, snapshot).unwrap();

        let orientation = self
            .player_movement
            .take_orientation(self.process_timer.elapsed());

        actor_orientation.update(orientation);

        let movement = self.player_movement.velocity(orientation);

        actor_velocity.update(Velocity { vector: movement });
    }
}
