use crate::entity::{
    actor::Actor,
    effect::Effect,
};
use std::collections::BTreeMap;

// Second member of tuple is target of the effect, third - source of the effect
pub struct StateEffectComponent(BTreeMap<(Effect, Actor, Option<Actor>), [u8; 16]>);

impl StateEffectComponent {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn get(&self, effect: Effect, actor: Actor, source: Option<Actor>) -> Option<&[u8; 16]> {
        self.0.get(&(effect, actor, source))
    }
}
