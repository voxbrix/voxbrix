use anyhow::Error;
use nohash_hasher::{
    IntMap,
    IntSet,
};
use rayon::prelude::*;
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    any::Any,
    collections::hash_map,
};
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::{
            ServerSnapshot,
            MAX_SNAPSHOT_DIFF,
        },
        update::Update,
    },
    messages::{
        ComponentPacker,
        UpdatesUnpacked,
    },
    pack,
    LabelLibrary,
};
use voxbrix_world::{
    Initialization,
    World,
};

pub mod class;
pub mod effect;
pub mod equipment;
pub mod movement_change;
pub mod movement_metadata;
pub mod orientation;
pub mod player;
pub mod position;
pub mod projectile;
pub mod velocity;

// pub struct Writable<'a, T> {
// actor: Actor,
// snapshot: Snapshot,
// changes: &'a mut IntMap<Actor, Snapshot>,
// data: &'a mut T,
// }
//
// impl<'a, T> Writable<'a, T>
// where
// T: PartialEq,
// {
// Only updates value if it is different from the old one.
// pub fn update(&mut self, value: T) {
// let Self {
// actor,
// snapshot,
// changes,
// data,
// } = self;
//
// if value != **data {
// data = value;
// changes.insert(*actor, *snapshot);
// }
// }
// }
//
// impl<'a, T> Deref for Writable<'a, T> {
// type Target = T;
//
// fn deref(&self) -> &T {
// self.data
// }
// }

/// Reusable, type-erased [`ComponentPacker`] storage for a single component.
#[derive(Default)]
pub struct ComponentPackerSlot(Option<Box<dyn Any + Send>>);

impl ComponentPackerSlot {
    /// Take the stored packer, or a fresh one if absent / of a different type.
    pub fn take<E, T>(&mut self) -> ComponentPacker<'static, E, T>
    where
        E: 'static + Send + Sync + Serialize,
        T: 'static + Send + Sync + Serialize,
    {
        self.0
            .take()
            .and_then(|b| b.downcast::<ComponentPacker<'static, E, T>>().ok())
            .map(|b| *b)
            .unwrap_or_default()
    }

    pub fn put<E, T>(&mut self, packer: ComponentPacker<'static, E, T>)
    where
        E: 'static + Send + Sync + Serialize,
        T: 'static + Send + Sync + Serialize,
    {
        self.0 = Some(Box::new(packer));
    }
}

/// Component that can be packed into State and distributed to clients.
pub trait ActorComponentPack: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn pack(
        &self,
        full_data: bool,
        client_last_snapshot: ServerSnapshot,
        player_actor: Option<&Actor>,
        actors_full_update: &IntSet<Actor>,
        actors_partial_update: &IntSet<Actor>,
        buffer: &mut Vec<u8>,
        packer_slot: &mut ComponentPackerSlot,
    ) -> Update;
}

/// Must be called once per tick before any per-client packing.
pub trait ActorComponentPreparePacking: Send {
    fn prepare_packing(&mut self, snapshot: ServerSnapshot);
}

/// Component that can be packed into State and distributed to clients
pub struct ActorComponentPackable<T>
where
    T: 'static,
{
    update: Update,
    last_packed_snapshot: ServerSnapshot,
    changes: IntMap<Actor, ServerSnapshot>,
    storage: IntMap<Actor, T>,
    /// Pre-mutation `(storage_value, changes_entry)` per actor, set on the
    /// first mutation each snapshot. Used by [`Self::prepare_packing`] to
    /// detect and suppress round-trip-canceled changes. Cleared on every
    /// `prepare_packing`.
    pre_snapshot: IntMap<Actor, (Option<T>, Option<ServerSnapshot>)>,
}

impl<'a, T> ActorComponentPackable<T>
where
    T: 'a + Deserialize<'a> + PartialEq,
{
    pub fn unpack_player(
        &mut self,
        player_actor: &Actor,
        updates: &UpdatesUnpacked<'a>,
        snapshot: ServerSnapshot,
    ) {
        if let Some((change, _)) = updates
            .get(&self.update)
            .and_then(pack::decode_from_slice::<Option<T>>)
        {
            let updated = if let Some(new_value) = change {
                let old_value = self.storage.get(player_actor);
                let updated = old_value != Some(&new_value);
                self.storage.insert(*player_actor, new_value);
                updated
            } else {
                self.storage.remove(player_actor).is_some()
            };

            if updated {
                self.changes.insert(*player_actor, snapshot);
            }
        }
    }
}

impl<T> ActorComponentPack for ActorComponentPackable<T>
where
    T: 'static + Serialize + PartialEq + Send + Sync,
{
    fn pack(
        &self,
        full_data: bool,
        client_last_snapshot: ServerSnapshot,
        player_actor: Option<&Actor>,
        actors_full_update: &IntSet<Actor>,
        actors_partial_update: &IntSet<Actor>,
        buffer: &mut Vec<u8>,
        packer_slot: &mut ComponentPackerSlot,
    ) -> Update {
        buffer.clear();
        let packer = packer_slot.take::<Actor, T>();

        let packer = if full_data {
            let iter = actors_full_update
                .iter()
                .filter(|actor| Some(*actor) != player_actor)
                .filter_map(|actor| Some((*actor, self.storage.get(actor)?)));

            packer.load_full(iter).pack(buffer)
        } else {
            let iter = actors_partial_update
                .iter()
                .filter_map(|actor| self.changes.get_key_value(actor))
                .filter(|(_, past_snapshot)| past_snapshot.0 > client_last_snapshot.0)
                .map(|(actor, _)| actor)
                .chain(actors_full_update.iter())
                .filter(|actor| Some(*actor) != player_actor)
                .map(|actor| (*actor, self.storage.get(actor)));

            packer.load_changes(iter).pack(buffer)
        };

        packer_slot.put(packer);

        self.update
    }
}

impl<T> ActorComponentPackable<T>
where
    T: PartialEq,
{
    /// On the first mutation of `actor` this snapshot, moves the prior
    /// `(storage, changes)` state into `pre_snapshot`. No-op on subsequent
    /// same-snapshot mutations.
    fn save_pre_snapshot(&mut self, actor: Actor) {
        if let hash_map::Entry::Vacant(slot) = self.pre_snapshot.entry(actor) {
            let prev_value = self.storage.remove(&actor);
            let prev_change = self.changes.get(&actor).copied();
            slot.insert((prev_value, prev_change));
        }
    }

    pub fn insert(&mut self, actor: Actor, new: T, snapshot: ServerSnapshot) {
        self.save_pre_snapshot(actor);

        self.storage.insert(actor, new);
        self.changes.insert(actor, snapshot);
    }

    // pub fn get_writable(&mut self, actor: &Actor, snapshot: Snapshot) -> Option<Writable<T>> {
    // Some(Writable {
    // actor: *i,
    // snapshot,
    // changes: &mut self.changes,
    // data: self.storage.get_mut(i)?,
    // })
    // }

    pub fn remove(&mut self, actor: &Actor, snapshot: ServerSnapshot) {
        if !self.storage.contains_key(actor) {
            return;
        }

        self.save_pre_snapshot(*actor);
        self.storage.remove(actor);
        self.changes.insert(*actor, snapshot);
    }
}

impl<T> ActorComponentPackable<T>
where
    T: 'static + PartialEq + Send + Sync,
{
    pub fn par_iter(&self) -> impl ParallelIterator<Item = (Actor, &T)> {
        self.storage.par_iter().map(|(k, v)| (*k, v))
    }
}

impl<T> ActorComponentPackable<T> {
    pub fn get(&self, actor: &Actor) -> Option<&T> {
        self.storage.get(actor)
    }
}

impl<T> ActorComponentPreparePacking for ActorComponentPackable<T>
where
    T: 'static + PartialEq + Send,
{
    fn prepare_packing(&mut self, snapshot: ServerSnapshot) {
        // Suppress round-trip-canceled changes: if current state matches the
        // pre-snapshot state, restore the prior `changes` entry.
        for (actor, (pre_value, prev_change_snapshot)) in self.pre_snapshot.drain() {
            let current = self.storage.get(&actor);
            if current == pre_value.as_ref() {
                match prev_change_snapshot {
                    Some(prev) => {
                        self.changes.insert(actor, prev);
                    },
                    None => {
                        self.changes.remove(&actor);
                    },
                }
            }
        }

        if snapshot.0 > self.last_packed_snapshot.0 {
            self.changes
                .retain(move |_, past_snapshot| snapshot.0 - past_snapshot.0 <= MAX_SNAPSHOT_DIFF);

            self.last_packed_snapshot = snapshot;
        }
    }
}

/// Internal component that is not shared with the client
pub struct ActorComponent<T> {
    storage: IntMap<Actor, T>,
}

impl<T> ActorComponent<T> {
    pub fn insert(&mut self, actor: Actor, new: T) -> Option<T> {
        self.storage.insert(actor, new)
    }

    pub fn get(&self, actor: &Actor) -> Option<&T> {
        self.storage.get(actor)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.storage.iter().map(|(&a, t)| (a, t))
    }

    pub fn remove(&mut self, actor: &Actor) -> Option<T> {
        self.storage.remove(actor)
    }
}

impl<T> ActorComponent<T>
where
    T: Send + Sync,
{
    /// Clears the component and refills it from the parallel iterator.
    pub fn replace_from_par_iter(&mut self, iter: impl ParallelIterator<Item = (Actor, T)>) {
        self.storage.clear();
        self.storage.par_extend(iter);
    }

    pub fn par_iter(&self) -> impl ParallelIterator<Item = (&Actor, &T)> {
        self.storage.par_iter()
    }
}

pub trait WithUpdate {
    const UPDATE: &str;
}

impl<T> WithUpdate for Option<T>
where
    T: WithUpdate,
{
    const UPDATE: &str = T::UPDATE;
}

impl<T> Initialization for ActorComponentPackable<T>
where
    T: WithUpdate + Serialize + PartialEq + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let update = world
            .get_resource_ref::<LabelLibrary>()
            .get::<Update>(T::UPDATE)
            .ok_or_else(|| anyhow::anyhow!("update with label \"{}\" is undefined", T::UPDATE))?;

        Ok(Self {
            update,
            last_packed_snapshot: ServerSnapshot(0),
            changes: IntMap::default(),
            storage: IntMap::default(),
            pre_snapshot: IntMap::default(),
        })
    }
}

impl<T> Initialization for ActorComponent<T>
where
    T: Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(_world: &World) -> Result<Self, Self::Error> {
        Ok(Self {
            storage: IntMap::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_component() -> ActorComponentPackable<u32> {
        ActorComponentPackable {
            update: Update(0),
            last_packed_snapshot: ServerSnapshot(0),
            changes: IntMap::default(),
            storage: IntMap::default(),
            pre_snapshot: IntMap::default(),
        }
    }

    #[test]
    fn insert_records_change() {
        let mut c = new_component();
        let actor = Actor(1);
        let snapshot = ServerSnapshot(1);

        c.insert(actor, 42, snapshot);
        c.prepare_packing(snapshot);

        assert_eq!(c.storage.get(&actor), Some(&42));
        assert_eq!(c.changes.get(&actor), Some(&snapshot));
    }

    #[test]
    fn remove_records_change() {
        let mut c = new_component();
        let actor = Actor(1);

        c.insert(actor, 42, ServerSnapshot(1));
        c.prepare_packing(ServerSnapshot(1));

        let snapshot = ServerSnapshot(2);
        c.remove(&actor, snapshot);
        c.prepare_packing(snapshot);

        assert_eq!(c.storage.get(&actor), None);
        assert_eq!(c.changes.get(&actor), Some(&snapshot));
    }

    #[test]
    fn insert_same_value_does_not_record_change() {
        let mut c = new_component();
        let actor = Actor(1);

        c.insert(actor, 42, ServerSnapshot(1));
        c.prepare_packing(ServerSnapshot(1));

        let snapshot = ServerSnapshot(2);
        c.insert(actor, 42, snapshot);
        c.prepare_packing(snapshot);

        assert_eq!(c.storage.get(&actor), Some(&42));
        // The previous change snapshot is preserved; no new entry at `snapshot`.
        assert_eq!(c.changes.get(&actor), Some(&ServerSnapshot(1)));
    }

    #[test]
    fn round_trip_within_snapshot_is_suppressed() {
        let mut c = new_component();
        let actor = Actor(1);

        c.insert(actor, 42, ServerSnapshot(1));
        c.prepare_packing(ServerSnapshot(1));

        let snapshot = ServerSnapshot(2);
        c.insert(actor, 100, snapshot);
        c.insert(actor, 42, snapshot);
        c.prepare_packing(snapshot);

        assert_eq!(c.storage.get(&actor), Some(&42));
        assert_eq!(c.changes.get(&actor), Some(&ServerSnapshot(1)));
    }

    #[test]
    fn insert_remove_within_snapshot_on_absent_actor_is_suppressed() {
        let mut c = new_component();
        let actor = Actor(1);
        let snapshot = ServerSnapshot(1);

        c.insert(actor, 42, snapshot);
        c.remove(&actor, snapshot);
        c.prepare_packing(snapshot);

        assert_eq!(c.storage.get(&actor), None);
        assert_eq!(c.changes.get(&actor), None);
    }

    #[test]
    fn remove_of_absent_actor_is_noop() {
        let mut c = new_component();
        let actor = Actor(1);

        c.remove(&actor, ServerSnapshot(1));
        c.prepare_packing(ServerSnapshot(1));

        assert_eq!(c.storage.get(&actor), None);
        assert_eq!(c.changes.get(&actor), None);
    }
}
