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
    collections::hash_map,
    mem,
};
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::{
            Snapshot,
            MAX_SNAPSHOT_DIFF,
        },
        state_component::StateComponent,
    },
    messages::{
        ActorStatePack,
        StatePacker,
        StateUnpacked,
    },
    pack,
};

pub mod chunk_activation;
pub mod class;
pub mod orientation;
pub mod player;
pub mod position;
pub mod projectile;
pub mod velocity;

enum LoadedData {
    Changes,
    Full,
    None,
}

struct ActorComponentPacker<'a, T> {
    loaded_data: LoadedData,
    data_changes: Vec<(Actor, Option<&'a T>)>,
    data_full: Vec<(Actor, &'a T)>,
}

impl<T> ActorComponentPacker<'static, T>
where
    T: Serialize,
{
    fn new() -> Self {
        Self {
            loaded_data: LoadedData::None,
            data_full: Vec::new(),
            data_changes: Vec::new(),
        }
    }

    fn load_changes<'a>(
        self,
        iter: impl Iterator<Item = (Actor, Option<&'a T>)>,
    ) -> ActorComponentPacker<'a, T> {
        let mut new = self;
        new.data_changes.extend(iter);
        new.loaded_data = LoadedData::Changes;
        new
    }

    fn load_full<'a>(
        self,
        iter: impl Iterator<Item = (Actor, &'a T)>,
    ) -> ActorComponentPacker<'a, T> {
        let mut new = self;
        new.data_full.extend(iter);
        new.loaded_data = LoadedData::Full;
        new
    }
}

impl<'a, T> ActorComponentPacker<'a, T>
where
    T: Serialize,
{
    fn pack(mut self, buffer: &mut Vec<u8>) -> ActorComponentPacker<'static, T> {
        match self.loaded_data {
            LoadedData::Changes => {
                let msg = ActorStatePack::Change(&self.data_changes);
                pack::encode_into(&msg, buffer);
            },
            LoadedData::Full => {
                let msg = ActorStatePack::Full(&self.data_full);
                pack::encode_into(&msg, buffer);
            },
            LoadedData::None => {
                panic!("no changes loaded");
            },
        }

        self.data_changes.clear();
        self.data_full.clear();
        self.loaded_data = LoadedData::None;

        // Safety: the `self.data` is `Vec` that contains references with lifetime `'a`.
        // It is the only field of the struct that utilizes the `'a` lifetime and since we
        // empty the `Vec` with `clear()` on the previous step, this `unsafe` should be sound.
        unsafe {
            mem::transmute::<ActorComponentPacker<'a, T>, ActorComponentPacker<'static, T>>(self)
        }
    }
}

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

/// Component that can be packed into State and distributed to clients
pub struct ActorComponentPackable<T>
where
    T: 'static,
{
    state_component: StateComponent,
    last_packed_snapshot: Snapshot,
    changes: IntMap<Actor, Snapshot>,
    packer: Option<ActorComponentPacker<'static, T>>,
    storage: IntMap<Actor, T>,
}

impl<'a, T> ActorComponentPackable<T>
where
    T: 'a + Deserialize<'a> + PartialEq,
{
    pub fn unpack_player(
        &mut self,
        player_actor: &Actor,
        state: &StateUnpacked<'a>,
        snapshot: Snapshot,
    ) {
        if let Some((change, _)) = state
            .get_component(&self.state_component)
            .and_then(|buf| pack::decode_from_slice::<Option<T>>(buf))
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

impl<T> ActorComponentPackable<T>
where
    T: 'static + Serialize + PartialEq,
{
    pub fn new(state_component: StateComponent) -> Self {
        Self {
            state_component,
            last_packed_snapshot: Snapshot(0),
            changes: IntMap::default(),
            packer: Some(ActorComponentPacker::new()),
            storage: IntMap::default(),
        }
    }

    pub fn pack_full(
        &mut self,
        state: &mut StatePacker,
        player_actor: Option<&Actor>,
        actors_full_update: &IntSet<Actor>,
    ) {
        let mut packer = self.packer.take().unwrap();

        let buffer = state.get_component_buffer(self.state_component);

        if let Some(player_actor) = player_actor {
            let iter = actors_full_update
                .iter()
                .filter(|actor| actor != &player_actor)
                .filter_map(|actor| Some((*actor, self.storage.get(actor)?)));

            packer = packer.load_full(iter).pack(buffer);
        } else {
            let iter = actors_full_update
                .iter()
                .filter_map(|actor| Some((*actor, self.storage.get(actor)?)));

            packer = packer.load_full(iter).pack(buffer);
        }

        self.packer = Some(packer);
    }

    pub fn pack_changes(
        &mut self,
        state: &mut StatePacker,
        snapshot: Snapshot,
        client_last_snapshot: Snapshot,
        player_actor: Option<&Actor>,
        actors_full_update: &IntSet<Actor>,
        actors_partial_update: &IntSet<Actor>,
    ) {
        if snapshot.0 > self.last_packed_snapshot.0 {
            self.changes
                .retain(move |_, past_snapshot| snapshot.0 - past_snapshot.0 <= MAX_SNAPSHOT_DIFF);

            self.last_packed_snapshot = snapshot;
        }

        let mut packer = self.packer.take().unwrap();

        let changed_actors_iter = actors_partial_update
            .iter()
            .filter_map(|actor| self.changes.get_key_value(actor))
            .filter(|(_, past_snapshot)| past_snapshot.0 > client_last_snapshot.0)
            .map(|(actor, _)| actor)
            .chain(actors_full_update.iter());

        let buffer = state.get_component_buffer(self.state_component);

        if let Some(player_actor) = player_actor {
            let iter = changed_actors_iter
                .filter(|actor| actor != &player_actor)
                .map(|actor| (*actor, self.storage.get(actor)));

            packer = packer.load_changes(iter).pack(buffer);
        } else {
            let iter = changed_actors_iter.map(|actor| (*actor, self.storage.get(actor)));

            packer = packer.load_changes(iter).pack(buffer);
        }

        self.packer = Some(packer);
    }

    pub fn insert(&mut self, actor: Actor, new: T, snapshot: Snapshot) -> Option<T> {
        let (changed, prev_value) = match self.storage.entry(actor) {
            hash_map::Entry::Occupied(mut slot) => {
                let prev_value = slot.insert(new);
                let value = slot.get();

                (&prev_value != value, Some(prev_value))
            },
            hash_map::Entry::Vacant(slot) => {
                slot.insert(new);

                (true, None)
            },
        };

        if changed {
            self.changes.insert(actor, snapshot);
        }

        prev_value
    }

    // pub fn get_writable(&mut self, actor: &Actor, snapshot: Snapshot) -> Option<Writable<T>> {
    // Some(Writable {
    // actor: *i,
    // snapshot,
    // changes: &mut self.changes,
    // data: self.storage.get_mut(i)?,
    // })
    // }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.storage.iter().map(|(k, v)| (*k, v))
    }

    pub fn remove(&mut self, actor: &Actor, snapshot: Snapshot) -> Option<T> {
        let removed = self.storage.remove(actor);

        if removed.is_some() {
            self.changes.insert(*actor, snapshot);
        }

        removed
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

/// Internal component that is not shared with the client
pub struct ActorComponent<T> {
    storage: IntMap<Actor, T>,
}

impl<T> ActorComponent<T> {
    pub fn new() -> Self {
        Self {
            storage: IntMap::default(),
        }
    }

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
