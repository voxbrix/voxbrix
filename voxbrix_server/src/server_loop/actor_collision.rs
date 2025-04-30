use crate::{
    component::action::handler::projectile::{
        Alteration,
        Condition,
        Source,
        Target,
        Trigger,
    },
    server_loop::data::SharedData,
};
use voxbrix_common::entity::actor::Actor;

fn condition_valid(sd: &SharedData, condition: &Condition, source: &Actor) -> bool {
    match condition {
        Condition::Always => true,
        Condition::SourceActorHasNoEffect(effect) => !sd.effect_ac.has_effect(*source, *effect),
        Condition::And(conditions) => conditions.iter().all(|c| condition_valid(sd, c, source)),
        Condition::Or(conditions) => conditions.iter().any(|c| condition_valid(sd, c, source)),
    }
}

impl SharedData {
    pub fn handle_block_collision(&mut self, actor: Actor) {
        let sd = self;

        let Some(proj_ac) = sd.projectile_ac.get(&actor) else {
            return;
        };

        for handler in proj_ac.handler_set.iter() {
            match handler.trigger {
                Trigger::AnyCollision | Trigger::BlockCollision => {},
                Trigger::ActorCollision => continue,
            }

            if !condition_valid(sd, &handler.condition, &actor) {
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

                        sd.effect_ac.add(target, *effect, source);
                    },
                    Alteration::RemoveSourceActorEffect { effect } => {
                        let actor = match proj_ac.source_actor {
                            Some(actor) => actor,
                            None => continue,
                        };
                        sd.effect_ac.remove_any_source(actor, *effect);
                    },
                    Alteration::RemoveSelf => {
                        sd.remove_queue.remove_actor(&actor);
                    },
                }
            }
        }
    }
}
