use voxbrix_common::entity::actor::Actor;

pub struct ProjectileActorCollision {
    pub projectile: Actor,
    pub target: Actor,
}

pub type ProjectileActorCollisions = Vec<ProjectileActorCollision>;
