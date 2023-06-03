use crate::{
    component::actor::ActorSubcomponent,
    entity::actor_model::ActorAnimation,
};
use std::time::Instant;

pub type AnimationStateActorComponent = ActorSubcomponent<ActorAnimation, AnimationState>;

pub struct AnimationState {
    pub start: Instant,
}
