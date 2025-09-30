use crate::component::actor::ActorComponent;
use voxbrix_common::component::actor::{
    position::Position,
    velocity::Velocity,
};

pub struct MovementChange {
    #[allow(dead_code)]
    pub prev_position: Position,
    pub next_position: Position,
    #[allow(dead_code)]
    pub prev_velocity: Velocity,
    pub next_velocity: Velocity,
    pub collides_with_block: bool,
}

pub type MovementChangeActorComponent = ActorComponent<MovementChange>;
