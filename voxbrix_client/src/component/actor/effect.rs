use std::collections::BTreeMap;
use voxbrix_common::{
    component::actor::effect::EffectState,
    entity::{
        actor::Actor,
        effect::{
            Effect,
            EffectDiscriminant,
        },
        update::Update,
    },
    math::MinMax,
    messages::{
        ComponentUpdateUnpack,
        UpdatesUnpacked,
    },
    pack,
};

pub struct EffectActorComponent {
    // First member of tuple is target of the effect, third - source of the effect
    storage: BTreeMap<(Actor, Effect, EffectDiscriminant), EffectState>,
    update: Update,
}

impl EffectActorComponent {
    pub fn new(update: Update) -> Self {
        Self {
            storage: BTreeMap::new(),
            update,
        }
    }

    #[allow(dead_code)]
    pub fn has_effect(&self, actor: Actor, effect: Effect) -> bool {
        self.storage
            .range(
                (actor, effect, EffectDiscriminant::MIN)
                    .. (actor, effect, EffectDiscriminant::MAX),
            )
            .next()
            .is_some()
    }

    #[allow(dead_code)]
    pub fn actor_effects(
        &self,
        actor: &Actor,
    ) -> impl DoubleEndedIterator<Item = (&(Actor, Effect, EffectDiscriminant), &EffectState)> {
        self.storage.range(
            (*actor, Effect::MIN, EffectDiscriminant::MIN)
                .. (*actor, Effect::MAX, EffectDiscriminant::MAX),
        )
    }

    pub fn unpack<'a>(&mut self, updates: &UpdatesUnpacked<'a>) {
        if let Some((changes, _)) = updates.get(&self.update).and_then(|buffer| {
            pack::decode_from_slice::<
                ComponentUpdateUnpack<(Actor, Effect, EffectDiscriminant), EffectState>,
            >(buffer)
        }) {
            match changes {
                ComponentUpdateUnpack::Change(changes) => {
                    for (key, change) in changes {
                        if let Some(change) = change {
                            self.storage.insert(key, change);
                        } else {
                            self.storage.remove(&key);
                        }
                    }
                },
                ComponentUpdateUnpack::Full(full) => {
                    self.storage.clear();
                    self.storage.extend(full);
                },
            }
        }
    }
}
