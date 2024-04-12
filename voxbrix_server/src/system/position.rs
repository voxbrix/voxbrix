use crate::component::{
    actor::{
        player::PlayerActorComponent,
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
    },
    block::class::ClassBlockComponent,
};
use std::time::Duration;
use voxbrix_common::{
    component::block_class::collision::CollisionBlockClassComponent,
    entity::snapshot::Snapshot,
    system::position,
};

pub struct PositionSystem;

impl PositionSystem {
    pub fn new() -> Self {
        Self
    }

    pub fn process(
        &mut self,
        dt: Duration,
        class_bc: &ClassBlockComponent,
        collision_bcc: &CollisionBlockClassComponent,
        position_ac: &mut PositionActorComponent,
        velocity_ac: &VelocityActorComponent,
        player_ac: &PlayerActorComponent,
        snapshot: Snapshot,
    ) {
        // TODO: replace
        let h_radius = 0.45;
        let v_radius = 0.95;
        let radius = [h_radius, h_radius, v_radius];

        for (actor, velocity) in velocity_ac
            .iter()
            .filter(|(actor, _)| player_ac.get(actor).is_none())
        {
            let mut position = match position_ac.get_writable(&actor, snapshot) {
                Some(v) => v,
                None => continue,
            };

            let new_pos =
                position::process_actor(dt, class_bc, collision_bcc, &position, velocity, &radius);

            position.update(new_pos);
        }
    }
}
