use crate::{
    component::{
        action::handler::projectile::{
            Alteration,
            Condition,
            Source,
            Target,
            Trigger,
        },
        actor::{
            position::PositionChanges,
            projectile::ProjectileActorComponent,
        },
    },
    resource::removal_queue::RemovalQueue,
};
use voxbrix_common::{
    component::actor::effect::EffectActorComponent,
    entity::actor::Actor,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct ActorBlockCollisionSystem;

impl System for ActorBlockCollisionSystem {
    type Data<'a> = ActorBlockCollisionSystemData<'a>;
}

#[derive(SystemData)]
pub struct ActorBlockCollisionSystemData<'a> {
    effect_ac: &'a mut EffectActorComponent,
    projectile_ac: &'a ProjectileActorComponent,
    position_changes: &'a PositionChanges,
    actor_rq: &'a mut RemovalQueue<Actor>,
}

impl ActorBlockCollisionSystemData<'_> {
    fn condition_valid(&self, condition: &Condition, source: &Actor) -> bool {
        match condition {
            Condition::Always => true,
            Condition::SourceActorHasNoEffect(effect) => {
                !self.effect_ac.has_effect(*source, *effect)
            },
            Condition::And(conditions) => {
                conditions.iter().all(|c| self.condition_valid(c, source))
            },
            Condition::Or(conditions) => conditions.iter().any(|c| self.condition_valid(c, source)),
        }
    }

    pub fn run(self) {
        for change in self
            .position_changes
            .iter()
            .filter(|c| c.collides_with_block)
        {
            let actor = change.actor;

            let Some(proj_ac) = self.projectile_ac.get(&actor) else {
                return;
            };

            for handler in proj_ac.handler_set.iter() {
                match handler.trigger {
                    Trigger::AnyCollision | Trigger::BlockCollision => {},
                    Trigger::ActorCollision => continue,
                }

                if !self.condition_valid(&handler.condition, &actor) {
                    continue;
                }

                for alteration in handler.alterations.iter() {
                    match alteration {
                        Alteration::ApplyEffect {
                            source,
                            target,
                            effect,
                        } => {
                            let source = match source {
                                Source::Source => proj_ac.source_actor,
                                Source::World => None,
                                Source::Collider => None,
                            };

                            let target = match target {
                                Target::Source => {
                                    match proj_ac.source_actor {
                                        Some(actor) => actor,
                                        None => continue,
                                    }
                                },
                                Target::AllInRadius(_radius) => unimplemented!(),
                                Target::Collider => continue,
                            };

                            self.effect_ac.add(target, *effect, source);
                        },
                        Alteration::RemoveSourceActorEffect { effect } => {
                            let actor = match proj_ac.source_actor {
                                Some(actor) => actor,
                                None => continue,
                            };
                            self.effect_ac.remove_any_source(actor, *effect);
                        },
                        Alteration::RemoveSelf => {
                            self.actor_rq.enqueue(actor);
                        },
                    }
                }
            }
        }
    }
}
