use crate::component::{
    action::handler::projectile::HandlerSet,
    actor::ActorComponent,
};
use voxbrix_common::entity::actor::Actor;

pub struct Projectile {
    pub source_actor: Option<Actor>,
    #[allow(dead_code)]
    pub action_data: Vec<u8>,
    pub handler_set: HandlerSet,
}

pub type ProjectileActorComponent = ActorComponent<Projectile>;
