use crate::{
    component::{
        action::handler::{
            initial,
            HandlerActionComponent,
        },
        actor::{
            class::ClassActorComponent,
            effect::EffectActorComponent,
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
            actor::ActorPlayerComponent,
            chunk_view::ChunkViewPlayerComponent,
            client::ClientPlayerComponent,
            dispatches_packer::DispatchesPackerPlayerComponent,
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
    EffectDiscriminantType,
    EffectStateType,
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
            effect::EffectState,
            velocity::Velocity,
        },
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::{
        action::Action,
        actor::Actor,
        effect::EffectDiscriminant,
        snapshot::ServerSnapshot,
    },
    messages::{
        server::ClientState,
        ClientActionsUnpacker,
    },
    pack,
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
    source: &'a Actor,
    action: &'a Action,
    effect_ac: &'a EffectActorComponent,
}

impl ConditionCheck<'_> {
    fn is_valid(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Always => true,
            Condition::SourceActorHasNoEffect {
                effect,
                discriminant,
            } => {
                let discriminant = match discriminant {
                    EffectDiscriminantType::None => EffectDiscriminant::none(),
                    EffectDiscriminantType::SourceActor => EffectDiscriminant(self.source.0 as u64),
                    EffectDiscriminantType::Action => EffectDiscriminant(self.action.0 as u64),
                };

                !self
                    .effect_ac
                    .has_effect(*self.source, *effect, discriminant)
            },
            Condition::And(conditions) => conditions.iter().all(|c| self.is_valid(c)),
            Condition::Or(conditions) => conditions.iter().any(|c| self.is_valid(c)),
        }
    }
}

pub enum Error {
    Corrupted,
    PlayerActorMissing,
    PlayerHasNoClient,
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
    dispatches_packer_pc: &'a mut DispatchesPackerPlayerComponent,
    chunk_view_pc: &'a ChunkViewPlayerComponent,
    class_bc: &'a mut ClassBlockComponent,
    collision_bcc: &'a CollisionBlockClassComponent,
}

impl PlayerActionsSystemData<'_> {
    pub fn run(mut self, player: Player, state: &ClientState) -> Result<(), Error> {
        let actions = self.actions_unpacker.unpack(&state.actions).map_err(|_| {
            debug!("unable to unpack actions");

            Error::Corrupted
        })?;

        let actor = *self.actor_pc.get(&player).ok_or_else(|| {
            error!("player actor is missing");

            Error::PlayerActorMissing
        })?;

        let client = self.client_pc.get(&player).ok_or_else(|| {
            error!("unable to find client for player");

            Error::PlayerHasNoClient
        })?;

        // Filtering out already handled actions
        for (action, _, data) in actions
            .data()
            .iter()
            .filter(|(_, snapshot, _)| *snapshot > client.last_client_snapshot)
        {
            let handler_set = self.handler_ac.get(action);

            for handler in handler_set.iter() {
                if !(ConditionCheck {
                    source: &actor,
                    action,
                    effect_ac: &self.effect_ac,
                }
                .is_valid(&handler.condition))
                {
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
                                    EffectDiscriminant(actor.0 as u64)
                                },
                                EffectDiscriminantType::Action => {
                                    EffectDiscriminant(action.0 as u64)
                                },
                            };

                            let mut state_buf = EffectState::new();

                            match state {
                                EffectStateType::None => {},
                                EffectStateType::CurrentSnapshot => {
                                    pack::encode_write(self.snapshot, &mut state_buf);
                                },
                            }

                            let target = match target {
                                Target::Source => actor,
                            };

                            self.effect_ac.insert(
                                target,
                                *effect,
                                discriminant,
                                state_buf,
                                *self.snapshot,
                            );
                        },
                        Alteration::RemoveSourceActorEffect { effect } => {
                            self.effect_ac.remove_any(actor, *effect, *self.snapshot);
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
                                dispatches_packer_pc: &mut self.dispatches_packer_pc,
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

        Ok(())
    }
}
