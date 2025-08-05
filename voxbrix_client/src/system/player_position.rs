use crate::{
    component::{
        actor::{
            position::PositionActorComponent,
            velocity::VelocityActorComponent,
            WritableTrait,
        },
        block::class::ClassBlockComponent,
    },
    resource::player_actor::PlayerActor,
};
use voxbrix_common::{
    component::block_class::collision::CollisionBlockClassComponent,
    entity::snapshot::ClientSnapshot,
    resource::process_timer::ProcessTimer,
    system::position,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PlayerPositionSystem;

impl System for PlayerPositionSystem {
    type Data<'a> = PlayerPositionSystemData<'a>;
}

#[derive(SystemData)]
pub struct PlayerPositionSystemData<'a> {
    snapshot: &'a ClientSnapshot,
    process_timer: &'a ProcessTimer,
    player_actor: &'a PlayerActor,
    class_bc: &'a ClassBlockComponent,
    collision_bcc: &'a CollisionBlockClassComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a VelocityActorComponent,
}

impl PlayerPositionSystemData<'_> {
    pub fn run(self) {
        // TODO: replace
        let h_radius = 0.45;
        let v_radius = 0.95;
        let radius = [h_radius, h_radius, v_radius];

        if let Some((velocity, mut writable_position)) =
            self.velocity_ac.get(&self.player_actor.0).zip(
                self.position_ac
                    .get_writable(&self.player_actor.0, *self.snapshot),
            )
        {
            let (new_pos, _new_vel) = position::process_actor(
                self.process_timer.elapsed(),
                self.class_bc,
                self.collision_bcc,
                &*writable_position,
                velocity,
                &radius,
                |_, _| {},
                |_, _| {},
            );

            writable_position.update(new_pos);
        }
    }
}
