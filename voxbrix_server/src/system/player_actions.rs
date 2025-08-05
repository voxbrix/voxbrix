use crate::{
    component::{
        action::handler::{
            initial,
            HandlerActionComponent,
        },
        actor::{
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            position::PositionActorComponent,
            projectile::{
                Projectile,
                ProjectileActorComponent,
            },
            velocity::VelocityActorComponent,
        },
        block::class::ClassBlockComponent,
        player::{
            actions_packer::ActionsPackerPlayerComponent,
            actor::ActorPlayerComponent,
            chunk_view::ChunkViewPlayerComponent,
            client::ClientPlayerComponent,
        },
    },
    entity::{
        actor::ActorRegistry,
        player::Player,
    },
    resource::script_shared_data::{
        ScriptSharedData,
        ScriptSharedDataRef,
    },
};
use initial::{
    Alteration,
    Condition,
    Source,
    Target,
};
use log::{
    debug,
    error,
};
use server_loop_api::ActionInput;
use voxbrix_common::{
    component::{
        actor::{
            effect::EffectActorComponent,
            velocity::Velocity,
        },
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::{
        actor::Actor,
        snapshot::ServerSnapshot,
    },
    messages::{
        server::ClientState,
        ClientActionsUnpacker,
    },
    script_registry::ScriptRegistry,
    LabelLibrary,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct PlayerActionsSystem;

impl System for PlayerActionsSystem {
    type Data<'a> = PlayerActionsSystemData<'a>;
}

struct ConditionCheck<'a> {
    effect_ac: &'a EffectActorComponent,
}

impl ConditionCheck<'_> {
    fn is_valid(&self, condition: &Condition, source: &Actor) -> bool {
        match condition {
            Condition::Always => true,
            Condition::SourceActorHasNoEffect(effect) => {
                !self.effect_ac.has_effect(*source, *effect)
            },
            Condition::And(conditions) => conditions.iter().all(|c| self.is_valid(c, source)),
            Condition::Or(conditions) => conditions.iter().any(|c| self.is_valid(c, source)),
        }
    }
}

#[derive(SystemData)]
pub struct PlayerActionsSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    actor_pc: &'a ActorPlayerComponent,
    client_pc: &'a ClientPlayerComponent,
    actions_unpacker: &'a mut ClientActionsUnpacker,

    script_registry: &'a mut ScriptRegistry<ScriptSharedData>,

    handler_ac: &'a HandlerActionComponent,

    effect_ac: &'a mut EffectActorComponent,

    actor_registry: &'a mut ActorRegistry,
    class_ac: &'a mut ClassActorComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
    projectile_ac: &'a mut ProjectileActorComponent,

    label_library: &'a LabelLibrary,
    actions_packer_pc: &'a mut ActionsPackerPlayerComponent,
    chunk_view_pc: &'a ChunkViewPlayerComponent,
    class_bc: &'a mut ClassBlockComponent,
    collision_bcc: &'a CollisionBlockClassComponent,
}

impl PlayerActionsSystemData<'_> {
    pub fn run(mut self, player: Player, state: &ClientState) {
        let actor = *self
            .actor_pc
            .get(&player)
            .expect("player missing actor must be caught earlier");

        let Ok(actions) = self.actions_unpacker.unpack_actions(&state.actions) else {
            debug!("unable to unpack actions");
            return;
        };

        let Some(client) = self.client_pc.get(&player) else {
            error!("unable to find client for player");
            return;
        };

        // Filtering out already handled actions
        for (action, _, data) in actions
            .data()
            .iter()
            .filter(|(_, snapshot, _)| *snapshot > client.last_client_snapshot)
        {
            let handler_set = self.handler_ac.get(action);

            for handler in handler_set.iter() {
                if !(ConditionCheck {
                    effect_ac: &self.effect_ac,
                }
                .is_valid(&handler.condition, &actor))
                {
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
                                Source::Actor => Some(actor),
                                Source::World => None,
                            };

                            let target = match target {
                                Target::Source => actor,
                            };

                            self.effect_ac.add(target, *effect, source);
                        },
                        Alteration::RemoveSourceActorEffect { effect } => {
                            self.effect_ac.remove_any_source(actor, *effect);
                        },
                        Alteration::CreateProjectile {
                            actor_class,
                            handler_set,
                            velocity_magnitude,
                        } => {
                            let Some(position) = self.position_ac.get(&actor).cloned() else {
                                continue;
                            };

                            // TODO automatically change orientation for projectiles
                            let Some(orientation) = self.orientation_ac.get(&actor).cloned() else {
                                continue;
                            };

                            let projectile = self.actor_registry.add(*self.snapshot);
                            self.class_ac
                                .insert(projectile, *actor_class, *self.snapshot);
                            self.projectile_ac.insert(
                                projectile,
                                Projectile {
                                    source_actor: Some(actor),
                                    action_data: data.to_vec(),
                                    handler_set: handler_set.clone(),
                                },
                            );
                            self.position_ac
                                .insert(projectile, position, *self.snapshot);
                            self.orientation_ac
                                .insert(projectile, orientation, *self.snapshot);
                            self.velocity_ac.insert(
                                projectile,
                                Velocity {
                                    vector: orientation.forward() * *velocity_magnitude,
                                },
                                *self.snapshot,
                            );
                        },
                        Alteration::Scripted { script } => {
                            let script_data = ScriptSharedDataRef {
                                snapshot: *self.snapshot,
                                actor_pc: &self.actor_pc,
                                actions_packer_pc: &mut self.actions_packer_pc,
                                chunk_view_pc: &self.chunk_view_pc,
                                position_ac: &self.position_ac,
                                label_library: &self.label_library,
                                class_bc: &mut self.class_bc,
                                collision_bcc: &self.collision_bcc,
                            }
                            .into_static();

                            self.script_registry.run_script(
                                &script,
                                script_data,
                                ActionInput {
                                    action: (*action).into(),
                                    actor: Some(actor.into()),
                                    data,
                                },
                            );
                        },
                    }
                }
            }
        }
    }
}
