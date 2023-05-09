use crate::{
    component::actor_model::ActorModelComponent,
    entity::actor_model::ActorAnimation,
};

pub type AnimationActorModelComponent = ActorModelComponent<ActorAnimation, ActorAnimationBuilder>;

pub struct ActorAnimationBuilder {}
