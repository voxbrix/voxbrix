use crate::component::{
    actor::{
        class::ClassActorComponent,
        player::PlayerActorComponent,
        position::PositionActorComponent,
        velocity::VelocityActorComponent,
    },
    actor_class::dimension_acceleration::DimensionAccelerationActorClassComponent,
};
use voxbrix_common::{
    component::{
        actor::velocity::Velocity,
        dimension_kind::acceleration::AccelerationDimensionKindComponent,
    },
    entity::snapshot::ServerSnapshot,
    resource::process_timer::ProcessTimer,
};
use voxbrix_world::{
    System,
    SystemData,
};

/// Server-side mirror of the client's `PlayerAccelerationSystem`,
/// applied to all non-player actors.
///
/// For each non-player actor it adds, to the actor's velocity,
/// the dimension-kind acceleration of the actor's current dimension,
/// scaled by the actor's per-class `DimensionAcceleration` scalar
/// and the elapsed frame time.
pub struct ActorAccelerationSystem;

impl System for ActorAccelerationSystem {
    type Data<'a> = ActorAccelerationSystemData<'a>;
}

#[derive(SystemData)]
pub struct ActorAccelerationSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    process_timer: &'a ProcessTimer,
    class_ac: &'a ClassActorComponent,
    player_ac: &'a PlayerActorComponent,
    position_ac: &'a PositionActorComponent,
    dimension_acceleration_acc: &'a DimensionAccelerationActorClassComponent,
    acceleration_dkc: &'a AccelerationDimensionKindComponent,
    velocity_ac: &'a mut VelocityActorComponent,
}

impl ActorAccelerationSystemData<'_> {
    pub fn run(self) {
        let dt = self.process_timer.elapsed();

        self.velocity_ac
            .par_for_each_mut(*self.snapshot, |actor, velocity| {
                if self.player_ac.get(&actor).is_some() {
                    return;
                }

                let Some(position) = self.position_ac.get(&actor) else {
                    return;
                };
                let Some(actor_class) = self.class_ac.get(&actor) else {
                    return;
                };

                let scalar = self
                    .dimension_acceleration_acc
                    .get(actor_class, &actor)
                    .0;

                let dv = self
                    .acceleration_dkc
                    .get(&position.chunk.dimension.kind)
                    .into_velocity(dt);

                *velocity = *velocity
                    + Velocity {
                        vector: dv.vector * scalar,
                    };
            });
    }
}
