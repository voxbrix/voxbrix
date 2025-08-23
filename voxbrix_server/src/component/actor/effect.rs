use crate::component::actor::MAX_SNAPSHOT_DIFF;
use nohash_hasher::IntSet;
use std::collections::{
    btree_map::Entry,
    BTreeMap,
    VecDeque,
};
use voxbrix_common::{
    component::actor::effect::EffectState,
    entity::{
        actor::Actor,
        effect::{
            Effect,
            EffectDiscriminant,
        },
        snapshot::ServerSnapshot,
        update::Update,
    },
    math::MinMax,
    messages::{
        ComponentPacker,
        UpdatesPacker,
    },
};

pub struct EffectActorComponent {
    // First member of tuple is target of the effect, third - source of the effect
    storage: BTreeMap<(Actor, Effect, EffectDiscriminant), EffectState>,
    changes: VecDeque<(ServerSnapshot, (Actor, Effect, EffectDiscriminant))>,
    packer: Option<ComponentPacker<'static, (Actor, Effect, EffectDiscriminant), EffectState>>,
    update: Update,
}

impl EffectActorComponent {
    pub fn new(update: Update) -> Self {
        Self {
            storage: BTreeMap::new(),
            changes: VecDeque::new(),
            packer: Some(ComponentPacker::new()),
            update,
        }
    }

    pub fn has_effect(
        &self,
        actor: Actor,
        effect: Effect,
        discriminant: EffectDiscriminant,
    ) -> bool {
        self.storage.contains_key(&(actor, effect, discriminant))
    }

    pub fn actor_effects(
        &self,
        actor: &Actor,
    ) -> impl DoubleEndedIterator<Item = (&(Actor, Effect, EffectDiscriminant), &EffectState)> {
        self.storage.range(
            (*actor, Effect::MIN, EffectDiscriminant::MIN)
                .. (*actor, Effect::MAX, EffectDiscriminant::MAX),
        )
    }

    pub fn insert(
        &mut self,
        actor: Actor,
        effect: Effect,
        discriminant: EffectDiscriminant,
        state: EffectState,
        snapshot: ServerSnapshot,
    ) {
        let key = (actor, effect, discriminant);

        match self.storage.entry(key) {
            Entry::Vacant(e) => {
                self.changes.push_back((snapshot, key));
                e.insert(state);
            },
            Entry::Occupied(mut e) => {
                let value = e.get_mut();

                if value != &state {
                    *value = state;

                    self.changes.push_back((snapshot, key));
                }
            },
        }
    }

    pub fn iter_mut(
        &mut self,
    ) -> impl ExactSizeIterator<Item = (&(Actor, Effect, EffectDiscriminant), &mut EffectState)>
    {
        self.storage.iter_mut()
    }

    pub fn remove(
        &mut self,
        actor: Actor,
        effect: Effect,
        discriminant: EffectDiscriminant,
        snapshot: ServerSnapshot,
    ) {
        let key = (actor, effect, discriminant);
        if self.storage.remove(&key).is_some() {
            self.changes.push_back((snapshot, key));
        }
    }

    /// Remove any instance of Effect.
    pub fn remove_any(&mut self, actor: Actor, effect: Effect, snapshot: ServerSnapshot) {
        while let Some(key) = self
            .storage
            .range((actor, Effect::MIN, EffectDiscriminant::MIN) ..)
            .next()
            .as_ref()
            .map(|kv| *kv.0)
        {
            if key.0 != actor || key.1 != effect {
                break;
            }

            self.storage.remove(&key);
            self.changes.push_back((snapshot, key));
        }
    }

    pub fn remove_actor(&mut self, actor: &Actor, snapshot: ServerSnapshot) {
        while let Some(key) = self
            .storage
            .range((*actor, Effect::MIN, EffectDiscriminant::MIN) ..)
            .next()
            .as_ref()
            .map(|kv| *kv.0)
        {
            if key.0 != *actor {
                break;
            }

            self.storage.remove(&key);
            self.changes.push_back((snapshot, key));
        }
    }

    pub fn pack_full(
        &mut self,
        updates_packer: &mut UpdatesPacker,
        actors_full_update: &IntSet<Actor>,
    ) {
        let mut packer = self.packer.take().unwrap();

        let buffer = updates_packer.get_buffer(self.update);

        let iter = actors_full_update
            .iter()
            .flat_map(|actor| self.actor_effects(actor))
            .map(|(k, v)| (*k, v));

        packer = packer.load_full(iter).pack(buffer);

        self.packer = Some(packer);
    }

    pub fn pack_changes(
        &mut self,
        updates_packer: &mut UpdatesPacker,
        snapshot: ServerSnapshot,
        last_confirmed_snapshot: ServerSnapshot,
        actors_full_update: &IntSet<Actor>,
        actors_partial_update: &IntSet<Actor>,
    ) {
        while let Some((change_snapshot, _)) = self.changes.front() {
            if snapshot.0 - change_snapshot.0 <= MAX_SNAPSHOT_DIFF {
                break;
            }

            self.changes.pop_front();
        }

        let mut packer = self.packer.take().unwrap();

        let full_changes_iter = actors_full_update
            .iter()
            .flat_map(|actor| self.actor_effects(actor))
            .map(|(k, v)| (k, Some(v)));

        let first_actual_change = self
            .changes
            .iter()
            .enumerate()
            .rev()
            .take_while(|(_, (snapshot, _))| snapshot > &last_confirmed_snapshot)
            .last();

        let partial_changes_iter = first_actual_change
            .iter()
            .flat_map(|(i, _)| self.changes.range(i ..))
            .filter(|(_, (actor, _, _))| actors_partial_update.contains(actor))
            .map(|(_, key)| (key, self.storage.get(key)));

        let changes_iter = full_changes_iter.chain(partial_changes_iter);

        let buffer = updates_packer.get_buffer(self.update);

        packer = packer
            .load_changes(changes_iter.map(|(k, v)| (*k, v)))
            .pack(buffer);

        self.packer = Some(packer);
    }
}
