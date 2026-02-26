use crate::{
    component::{
        actor::{
            effect::EffectActorComponent,
            position::PositionActorComponent,
        },
        block::class::ClassBlockComponent,
        dimension_kind::player_chunk_view::PlayerChunkViewDimensionKindComponent,
        effect::snapshot_handler::{
            Alteration,
            Condition,
            SnapshotHandlerEffectComponent,
        },
        player::{
            actor::ActorPlayerComponent,
            dispatches_packer::DispatchesPackerPlayerComponent,
        },
    },
    resource::script_shared_data::{
        ScriptSharedData,
        ScriptSharedDataRef,
    },
};
use voxbrix_common::{
    component::{
        actor::effect::EffectState,
        block_class::collision::CollisionBlockClassComponent,
    },
    entity::snapshot::ServerSnapshot,
    pack,
    script_registry::ScriptRegistry,
    LabelLibrary,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct EffectSnapshotSystem;

impl System for EffectSnapshotSystem {
    type Data<'a> = EffectSnapshotSystemData<'a>;
}

struct ConditionCheck<'a> {
    snapshot: &'a ServerSnapshot,
    effect_state: &'a EffectState,
}

impl ConditionCheck<'_> {
    fn is_valid(&self, condition: &Condition) -> bool {
        match condition {
            Condition::Always => true,
            Condition::EveryNSnapshot => {
                let (snapshot, bytes_used) =
                    pack::decode_from_slice::<ServerSnapshot>(self.effect_state)
                        .expect("unable to decode snapshot from effect state");

                let (duration, _) =
                    pack::decode_from_slice::<u32>(&self.effect_state[bytes_used ..])
                        .expect("unable to decode duration from effect state");

                self.snapshot.0.saturating_sub(snapshot.0) % (duration as u64) == 0
                    && *self.snapshot != snapshot
            },
            Condition::And(conditions) => conditions.iter().all(|c| self.is_valid(c)),
            Condition::Or(conditions) => conditions.iter().any(|c| self.is_valid(c)),
        }
    }
}

#[derive(SystemData)]
pub struct EffectSnapshotSystemData<'a> {
    snapshot: &'a ServerSnapshot,
    actor_pc: &'a ActorPlayerComponent,

    script_registry: &'a mut ScriptRegistry<ScriptSharedData>,

    effect_ac: &'a mut EffectActorComponent,

    position_ac: &'a mut PositionActorComponent,

    snapshot_handler_ec: &'a mut SnapshotHandlerEffectComponent,

    label_library: &'a LabelLibrary,
    dispatches_packer_pc: &'a mut DispatchesPackerPlayerComponent,
    class_bc: &'a mut ClassBlockComponent,
    collision_bcc: &'a CollisionBlockClassComponent,

    player_chunk_view_dkc: &'a PlayerChunkViewDimensionKindComponent,
}

impl EffectSnapshotSystemData<'_> {
    pub fn run(&mut self) {
        let mut effects_remove = Vec::new();
        // Filtering out already handled actions
        for (&(actor, effect, discriminant), effect_state) in self.effect_ac.iter_mut() {
            let handler_set = self.snapshot_handler_ec.get(&effect);

            for handler in handler_set.iter() {
                if !(ConditionCheck {
                    snapshot: self.snapshot,
                    effect_state,
                }
                .is_valid(&handler.condition))
                {
                    continue;
                }

                for alteration in handler.alterations.iter() {
                    match alteration {
                        Alteration::RemoveThisEffect => {
                            effects_remove.push((actor, effect, discriminant));
                        },
                        Alteration::Scripted { script } => {
                            let script_data = ScriptSharedDataRef {
                                snapshot: *self.snapshot,
                                actor_pc: self.actor_pc,
                                dispatches_packer_pc: self.dispatches_packer_pc,
                                player_chunk_view_dkc: self.player_chunk_view_dkc,
                                position_ac: self.position_ac,
                                label_library: self.label_library,
                                class_bc: self.class_bc,
                                collision_bcc: self.collision_bcc,
                            }
                            .into_static();

                            self.script_registry.run_script(
                                script,
                                script_data,
                                // FIXME special input for effect scripts.
                                (),
                            );
                        },
                    }
                }
            }
        }

        for (actor, effect, discriminant) in effects_remove {
            self.effect_ac
                .remove(actor, effect, discriminant, *self.snapshot);
        }
    }
}
