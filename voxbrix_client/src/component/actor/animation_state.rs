use crate::entity::actor_model::ActorAnimation;
use crate::component::actor::ActorSubcomponent;
use std::time::Instant;

pub type AnimationStateActorComponent = ActorSubcomponent<ActorAnimation, AnimationState>;

pub enum AnimationState {
    Inactive,
    Active {
        start: Instant,
    },
}
