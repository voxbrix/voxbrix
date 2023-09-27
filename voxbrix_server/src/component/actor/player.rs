use crate::{
    component::actor::ActorComponent,
    entity::player::Player,
};

// Marks player-owned actors.
pub type PlayerActorComponent = ActorComponent<Player>;
