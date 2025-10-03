use crate::component::{
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
        movement_change::MovementChangeActorComponent,
        projectile::ProjectileActorComponent,
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

pub struct ProjectileBlockHandlingSystem;

impl System for ProjectileBlockHandlingSystem {
    type Data<'a> = ProjectileBlockHandlingSystemData<'a>;
}

#[derive(SystemData)]
pub struct ProjectileBlockHandlingSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    effect_ac: &'a mut EffectActorComponent,
    projectile_ac: &'a ProjectileActorComponent,
    movement_changes: &'a MovementChangeActorComponent,
    actor_rq: &'a mut RemovalQueue<Actor>,
}

impl ProjectileBlockHandlingSystemData<'_> {
    fn condition_valid(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Always => true,
            Condition::And(conditions) => conditions.iter().all(|c| self.condition_valid(c)),
            Condition::Or(conditions) => conditions.iter().any(|c| self.condition_valid(c)),
        }
    }

    pub fn run(self) {
        for (proj_actor, _) in self
            .movement_changes
            .iter()
            .filter(|(_, c)| c.collides_with_block)
        {
            let Some(proj_ac) = self.projectile_ac.get(&proj_actor) else {
                continue;
            };

            for handler in proj_ac.handler_set.iter() {
                match handler.trigger {
                    Trigger::AnyCollision | Trigger::BlockCollision => {},
                    Trigger::ActorCollision => continue,
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
                            self.actor_rq.enqueue(proj_actor);
                        },
                    }
                }
            }
        }
    }
}
