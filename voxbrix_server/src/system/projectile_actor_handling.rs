use crate::{
    component::{
        action::handler::projectile::{
            Alteration,
            Condition,
            EffectDiscriminantType,
            EffectStateType,
            Target,
            Trigger,
        },
        actor::{
            effect::EffectActorComponent,
            projectile::ProjectileActorComponent,
        },
    },
    resource::projectile_actor_collisions::{
        ProjectileActorCollision,
        ProjectileActorCollisions,
    },
};
use voxbrix_common::{
    component::actor::effect::EffectState,
    entity::{
        actor::Actor,
        effect::EffectDiscriminant,
        snapshot::ServerSnapshot,
    },
    pack,
    resource::removal_queue::RemovalQueue,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ProjectileActorHandlingSystem;

impl System for ProjectileActorHandlingSystem {
    type Data<'a> = ProjectileActorHandlingSystemData<'a>;
}

#[derive(SystemData)]
pub struct ProjectileActorHandlingSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    effect_ac: &'a mut EffectActorComponent,
    projectile_ac: &'a ProjectileActorComponent,
    projectile_actor_collisions: &'a ProjectileActorCollisions,
    actor_rq: &'a mut RemovalQueue<Actor>,
}

impl ProjectileActorHandlingSystemData<'_> {
    fn condition_valid(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Always => true,
            Condition::And(conditions) => conditions.iter().all(|c| self.condition_valid(c)),
            Condition::Or(conditions) => conditions.iter().any(|c| self.condition_valid(c)),
        }
    }

    pub fn run(self) {
        for collision in self.projectile_actor_collisions.iter() {
            let ProjectileActorCollision {
                projectile: proj_actor,
                target: _targ_actor,
            } = collision;

            let Some(proj_ac) = self.projectile_ac.get(&proj_actor) else {
                return;
            };

            for handler in proj_ac.handler_set.iter() {
                match handler.trigger {
                    Trigger::AnyCollision | Trigger::ActorCollision => {},
                    Trigger::BlockCollision => continue,
                }

                if !self.condition_valid(&handler.condition) {
                    continue;
                }

                for alteration in handler.alterations.iter() {
                    match alteration {
                        Alteration::ApplyEffect {
                            target,
                            effect,
                            discriminant,
                            state,
                        } => {
                            let discriminant = match discriminant {
                                EffectDiscriminantType::None => EffectDiscriminant::none(),
                                EffectDiscriminantType::SourceActor => {
                                    let Some(source_actor) = proj_ac.source_actor else {
                                        continue;
                                    };
                                    EffectDiscriminant(source_actor.0 as u64)
                                },
                            };

                            let mut state_buf = EffectState::new();

                            match state {
                                EffectStateType::None => {},
                                EffectStateType::CurrentSnapshotWithN { n } => {
                                    pack::encode_write(self.snapshot, &mut state_buf);
                                    pack::encode_write(n, &mut state_buf);
                                },
                            }

                            let target = match target {
                                Target::Source => {
                                    match proj_ac.source_actor {
                                        Some(actor) => actor,
                                        None => continue,
                                    }
                                },
                                Target::Collider => continue,
                            };

                            self.effect_ac.insert(
                                target,
                                *effect,
                                discriminant,
                                Default::default(),
                                *self.snapshot,
                            );
                        },
                        Alteration::RemoveSourceActorEffect { effect } => {
                            let Some(source_actor) = proj_ac.source_actor else {
                                continue;
                            };
                            self.effect_ac
                                .remove_any(source_actor, *effect, *self.snapshot);
                        },
                        Alteration::RemoveSelf => {
                            self.actor_rq.enqueue(*proj_actor);
                        },
                    }
                }
            }
        }
    }
}
