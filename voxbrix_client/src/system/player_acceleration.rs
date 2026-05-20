use crate::{
    component::actor::{
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
        WritableTrait,
    },
    resource::player_actor::PlayerActor,
};
use voxbrix_common::{
    component::dimension_kind::acceleration::AccelerationDimensionKindComponent,
    entity::snapshot::ClientSnapshot,
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
    acceleration_dkc: &'a AccelerationDimensionKindComponent,
    velocity_ac: &'a mut VelocityActorComponent,
}

impl PlayerAccelerationSystemData<'_> {
    pub fn run(self) {
        if let Some((mut writable_velocity, position)) = self
            .velocity_ac
            .get_writable(&self.player_actor.0, *self.snapshot)
            .zip(self.position_ac.get(&self.player_actor.0))
        {
            let dv = self
                .acceleration_dkc
                .get(&position.chunk.dimension.kind)
                .into_velocity(self.process_timer.elapsed());

            let new_velocity = *writable_velocity + dv;

            writable_velocity.update(new_velocity);
        }
    }
}
