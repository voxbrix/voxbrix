use crate::{
    entity::{
        actor::Actor,
        effect::Effect,
    },
    math::MinMax,
};
use std::collections::BTreeSet;

// First member of tuple is target of the effect, third - source of the effect
pub struct EffectActorComponent(BTreeSet<(Actor, Effect, Option<Actor>)>);

impl EffectActorComponent {
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    pub fn has_effect(&self, actor: Actor, effect: Effect) -> bool {
        self.0
            .range((actor, effect, None) .. (actor, effect, Some(Actor::MAX)))
            .next()
            .is_some()
    }

    pub fn add(&mut self, actor: Actor, effect: Effect, source: Option<Actor>) {
        self.0.insert((actor, effect, source));
    }

    pub fn remove_any_source(&mut self, actor: Actor, effect: Effect) {
        while let Some(key) = self.0.range((actor, Effect::MIN, None) ..).next().copied() {
            if key.0 != actor || key.1 != effect {
                break;
            }

            self.0.remove(&key);
        }
    }

    pub fn remove_actor(&mut self, actor: &Actor) {
        while let Some(key) = self.0.range((*actor, Effect::MIN, None) ..).next().copied() {
            if key.0 != *actor {
                break;
            }

            self.0.remove(&key);
        }
    }
}
