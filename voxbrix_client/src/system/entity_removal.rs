use crate::component::actor::{
    animation_state::AnimationStateActorComponent,
    class::ClassActorComponent,
    orientation::OrientationActorComponent,
    position::PositionActorComponent,
    target_orientation::TargetOrientationActorComponent,
    target_position::TargetPositionActorComponent,
    velocity::VelocityActorComponent,
};
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::ClientSnapshot,
    },
    resource::removal_queue::RemovalQueue,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct EntityRemovalCheckSystem;

impl System for EntityRemovalCheckSystem {
    type Data<'a> = EntityRemovalCheckSystemData<'a>;
}

#[derive(SystemData)]
pub struct EntityRemovalCheckSystemData<'a> {
    actor_rq: &'a mut RemovalQueue<Actor>,
}

impl EntityRemovalCheckSystemData<'_> {
    pub fn run(self) -> bool {
        !self.actor_rq.is_empty()
    }
}

pub struct EntityRemovalSystem;

impl System for EntityRemovalSystem {
    type Data<'a> = EntityRemovalSystemData<'a>;
}

#[derive(SystemData)]
pub struct EntityRemovalSystemData<'a> {
    snapshot: &'a ClientSnapshot,

    actor_rq: &'a mut RemovalQueue<Actor>,
    class_ac: &'a mut ClassActorComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
    animation_state_ac: &'a mut AnimationStateActorComponent,
    target_position_ac: &'a mut TargetPositionActorComponent,
    target_orientation_ac: &'a mut TargetOrientationActorComponent,
}

impl EntityRemovalSystemData<'_> {
    pub fn run(self) {
        let snapshot = *self.snapshot;

        for actor in self.actor_rq.drain() {
            self.class_ac.remove(&actor, snapshot);
            self.position_ac.remove(&actor);
            self.velocity_ac.remove(&actor, snapshot);
            self.orientation_ac.remove(&actor, snapshot);
            self.animation_state_ac.remove_actor(&actor);
            self.target_position_ac.remove(&actor);
            self.target_orientation_ac.remove(&actor);
        }
    }
}
