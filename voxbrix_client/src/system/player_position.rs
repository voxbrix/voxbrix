use crate::component::{
    actor::{
        orientation::OrientationActorComponent,
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
    },
    block::class::ClassBlockComponent,
};
use std::time::Duration;
use voxbrix_common::{
    component::{
        actor::position::Position,
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::{
        actor::Actor,
        block::Block,
        chunk::Chunk,
        snapshot::Snapshot,
    },
    math::Vec3F32,
    system::position,
};

pub struct PlayerPositionSystem {
    player_actor: Actor,
}

impl PlayerPositionSystem {
    pub fn new(player_actor: Actor) -> Self {
        Self { player_actor }
    }

    pub fn process(
        &mut self,
        dt: Duration,
        class_bc: &ClassBlockComponent,
        collision_bcc: &CollisionBlockClassComponent,
        position_ac: &mut PositionActorComponent,
        velocity_ac: &VelocityActorComponent,
        snapshot: Snapshot,
    ) {
        // TODO: replace
        let h_radius = 0.45;
        let v_radius = 0.95;
        let radius = [h_radius, h_radius, v_radius];

        if let Some((velocity, mut writable_position)) = velocity_ac
            .get(&self.player_actor)
            .zip(position_ac.get_writable(&self.player_actor, snapshot))
        {
            let new_pos = position::process_actor(
                dt,
                class_bc,
                collision_bcc,
                &writable_position,
                velocity,
                &radius,
            );

            writable_position.update(new_pos);
        }
    }

    pub fn get_target_block(
        &self,
        position_ac: &PositionActorComponent,
        orientation_ac: &OrientationActorComponent,
        targeting: impl FnMut(Chunk, Block) -> bool,
    ) -> Option<(Chunk, Block, usize)> {
        position_ac
            .get(&self.player_actor)
            .zip(orientation_ac.get(&self.player_actor))
            .and_then(|(position, orientation)| {
                position::get_target_block(position, orientation.forward(), targeting)
            })
    }

    pub fn position_direction(
        &self,
        position_ac: &PositionActorComponent,
        orientation_ac: &OrientationActorComponent,
    ) -> (Position, Vec3F32) {
        position_ac
            .get(&self.player_actor)
            .copied()
            .zip(
                orientation_ac
                    .get(&self.player_actor)
                    .map(|ori| ori.forward()),
            )
            .expect("unable to get player orientation")
    }
}
