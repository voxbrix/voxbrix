use crate::{
    component::{
        actor::{
            class::ClassActorComponent,
            movement_change::MovementChangeActorComponent,
            position::PositionActorComponent,
            projectile::ProjectileActorComponent,
        },
        actor_class::hitbox::{
            Hitbox,
            HitboxActorClassComponent,
        },
    },
    resource::projectile_actor_collisions::{
        ProjectileActorCollision,
        ProjectileActorCollisions,
    },
};
use rayon::prelude::*;
use voxbrix_common::{
    entity::block::BLOCKS_IN_CHUNK_EDGE_F32,
    math::Vec3I32,
};
use voxbrix_world::{
    System,
    SystemData,
};

// Only calculate collision between projectiles and targets within N chunk radius of the chunk
// difference:
const COLLISION_CHUNK_RADIUS: i32 = 2;

pub struct ProjectileHitboxCollisionSystem;

impl System for ProjectileHitboxCollisionSystem {
    type Data<'a> = ProjectileHitboxCollisionSystemData<'a>;
}

#[derive(SystemData)]
pub struct ProjectileHitboxCollisionSystemData<'a> {
    class_ac: &'a ClassActorComponent,
    position_ac: &'a PositionActorComponent,
    movement_change_ac: &'a MovementChangeActorComponent,
    projectile_ac: &'a ProjectileActorComponent,
    hitbox_acc: &'a HitboxActorClassComponent,
    projectile_collisions: &'a mut ProjectileActorCollisions,
}

impl ProjectileHitboxCollisionSystemData<'_> {
    pub fn run(self) {
        self.projectile_collisions.clear();

        let par_iter = self
            .projectile_ac
            .par_iter()
            .filter_map(|(actor, projectile)| {
                let mc = self.movement_change_ac.get(actor)?;

                Some((actor, projectile, mc))
            })
            .flat_map_iter(move |(proj_actor, projectile, proj_movement_change)| {
                // TODO we might also want to filter out by AABB before trajectory calculation.
                // TODO ignore source actors for collision detection.
                proj_movement_change
                    .prev_position
                    .chunk
                    .radius_for_range(
                        &proj_movement_change.next_position.chunk,
                        COLLISION_CHUNK_RADIUS,
                    )
                    .unwrap_or_else(|| {
                        proj_movement_change
                            .next_position
                            .chunk
                            .radius(COLLISION_CHUNK_RADIUS)
                    })
                    .into_iter_simple()
                    .flat_map(|chunk| self.position_ac.get_actors_in_chunk(chunk))
                    .filter_map(move |targ_actor| {
                        if &targ_actor == proj_actor
                            || projectile
                                .source_actor
                                .map(|sa| targ_actor == sa)
                                .unwrap_or(false)
                        {
                            return None;
                        }

                        let targ_class = self.class_ac.get(&targ_actor)?;
                        let prev_targ_pos = self.position_ac.get(&targ_actor)?;
                        let hitbox = self.hitbox_acc.get(targ_class, &targ_actor)?;

                        let next_targ_pos = self
                            .movement_change_ac
                            .get(&targ_actor)
                            .map(|mc| &mc.prev_position)
                            .unwrap_or(prev_targ_pos);

                        let prev_proj_pos = proj_movement_change.prev_position;
                        let next_proj_pos = proj_movement_change.next_position;

                        let targ_pos_diff = Vec3I32::from_array(next_targ_pos.chunk.position)
                            .saturating_sub(Vec3I32::from_array(prev_targ_pos.chunk.position))
                            .as_vec3()
                            * BLOCKS_IN_CHUNK_EDGE_F32
                            + next_targ_pos.offset
                            - prev_targ_pos.offset;

                        let proj_pos_diff = Vec3I32::from_array(next_proj_pos.chunk.position)
                            .saturating_sub(Vec3I32::from_array(prev_proj_pos.chunk.position))
                            .as_vec3()
                            * BLOCKS_IN_CHUNK_EDGE_F32
                            + next_proj_pos.offset
                            - prev_proj_pos.offset;

                        // Relative velocity of the target from projectile's PoV,
                        // time is assumed to be 1:
                        let targ_rel_vel = targ_pos_diff - proj_pos_diff;

                        let targ_rel_pos = Vec3I32::from_array(prev_targ_pos.chunk.position)
                            .saturating_sub(Vec3I32::from_array(prev_proj_pos.chunk.position))
                            .as_vec3()
                            * BLOCKS_IN_CHUNK_EDGE_F32
                            + prev_targ_pos.offset
                            - prev_proj_pos.offset;

                        let collision_dist = match (&hitbox, &projectile.hitbox) {
                            (
                                Hitbox::Sphere { radius_blocks: r1 },
                                Hitbox::Sphere { radius_blocks: r2 },
                            ) => r1 + r2,
                        };

                        // vx^2 + vy^2 + vz^2
                        let a = targ_rel_vel.dot(targ_rel_vel);

                        // 2 * (vx*x0 + vy*y0 + vz*z0)
                        let b = 2.0 * targ_rel_vel.dot(targ_rel_pos);

                        // x0^2 + y0^2 + z0^2
                        let c = targ_rel_pos.dot(targ_rel_pos);

                        let d = b.powi(2) - 4.0 * a * (c - collision_dist.powi(2));

                        if d >= 0.0 {
                            let t1 = (-b - d.sqrt()) / 2.0 / a;
                            let t2 = (-b + d.sqrt()) / 2.0 / a;

                            // Third OR is true when either proj or targ are inside one another:
                            if (0.0 .. 1.0).contains(&t1)
                                || (0.0 .. 1.0).contains(&t2)
                                || t1 <= 0.0 && t2 >= 1.0
                            {
                                Some(ProjectileActorCollision {
                                    projectile: *proj_actor,
                                    target: targ_actor,
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
            });

        self.projectile_collisions.par_extend(par_iter);
    }
}
