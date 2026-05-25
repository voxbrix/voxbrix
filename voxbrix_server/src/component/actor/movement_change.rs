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
    /// Which sides of the actor's collision box collided with blocks,
    /// indexed as `[x_neg, x_pos, y_neg, y_pos, z_neg, z_pos]`.
    pub collision_sides: [bool; 6],
}

pub type MovementChangeActorComponent = ActorComponent<MovementChange>;
