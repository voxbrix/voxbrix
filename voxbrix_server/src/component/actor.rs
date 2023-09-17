use nohash_hasher::{
    IntMap,
    IntSet,
};
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::{
    mem,
    ops::Deref,
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
        State,
        StatePacker,
    },
    pack,
};

pub mod chunk_ticket;
pub mod class;
pub mod orientation;
pub mod position;
pub mod velocity;

struct ActorComponentPacker<'a, T> {
    data: Vec<(Actor, Option<&'a T>)>,
}

impl<T> ActorComponentPacker<'static, T>
where
    T: Serialize,
{
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn load<'a>(
        self,
        iter: impl Iterator<Item = (Actor, Option<&'a T>)>,
    ) -> ActorComponentPacker<'a, T> {
        let mut new = self;
        new.data.extend(iter);
        new
    }
}

impl<'a, T> ActorComponentPacker<'a, T>
where
    T: Serialize,
{
    fn pack(mut self, buffer: &mut Vec<u8>) -> ActorComponentPacker<'static, T> {
        pack::serialize_into(&self.data, buffer);

        self.data.clear();

        // Safety: the `self.data` is `Vec` that contains references with lifetime `'a`.
        // It is the only field of the struct that utilizes the `'a` lifetime and since we
        // empty the `Vec` with `clear()` on the previous step, this `unsafe` should be sound.
        unsafe {
            mem::transmute::<ActorComponentPacker<'a, T>, ActorComponentPacker<'static, T>>(self)
        }
    }
}

pub struct Writable<'a, T> {
    actor: Actor,
    snapshot: Snapshot,
    changes: &'a mut IntMap<Actor, Snapshot>,
    data: &'a mut T,
}

impl<'a, T> Writable<'a, T>
where
    T: PartialEq,
{
    /// Only updates value if it is different from the old one.
    pub fn update(&mut self, value: T) {
        let Self {
            actor,
            snapshot,
            changes,
            data,
        } = self;

        if value != **data {
            **data = value;
            changes.insert(*actor, *snapshot);
        }
    }
}

impl<'a, T> Deref for Writable<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.data
    }
}

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

impl<T> ActorComponentPackable<T>
where
    T: 'static + DeserializeOwned + PartialEq,
{
    pub fn unpack_player(&mut self, player_actor: &Actor, state: &State, snapshot: Snapshot) {
        if let Some(change) = state
            .get_component(&self.state_component)
            .and_then(|buf| pack::deserialize_from::<Option<T>>(buf))
        {
            let updated = if let Some(new_value) = change {
                let old_value = self.storage.get(player_actor);
                let updated = old_value != Some(&new_value);
                self.storage.insert(*player_actor, new_value);
                updated
            } else {
                self.storage.remove(player_actor);
                true
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

    pub fn pack_changes(
        &mut self,
        state: &mut StatePacker,
        snapshot: Snapshot,
        client_last_snapshot: Snapshot,
        player_actor: &Actor,
        mut actor_filter_fn: impl FnMut(&Actor) -> bool,
        actors_full_update: &IntSet<Actor>,
    ) {
        if snapshot.0 > self.last_packed_snapshot.0 {
            self.changes
                .retain(move |_, past_snapshot| snapshot.0 - past_snapshot.0 <= MAX_SNAPSHOT_DIFF);

            self.last_packed_snapshot = snapshot;
        }

        // if snapshot.0 - client_last_snapshot.0 > MAX_SNAPSHOT_DIFF {
        // TODO SEND ALL FOR DIFF > MAX_SNAPSHOT_DIFF
        //}

        let mut packer = self.packer.take().unwrap();

        let changes_iter = self
            .changes
            .iter()
            .filter(|(actor, past_snapshot)| {
                past_snapshot.0 > client_last_snapshot.0
                    && actor_filter_fn(actor)
                    && !actors_full_update.contains(actor)
            })
            .map(|(actor, _)| actor)
            .chain(actors_full_update.iter())
            .filter(|actor| actor != &player_actor)
            .map(|actor| (*actor, self.storage.get(actor)));

        let buffer = state.get_component_buffer(self.state_component);

        packer = packer.load(changes_iter).pack(buffer);

        self.packer = Some(packer);
    }

    pub fn insert(&mut self, i: Actor, new: T, snapshot: Snapshot) -> Option<T> {
        if Some(&new) != self.storage.get(&i) {
            self.changes.insert(i, snapshot);
        }

        self.storage.insert(i, new)
    }

    pub fn get(&self, i: &Actor) -> Option<&T> {
        self.storage.get(i)
    }

    pub fn get_writable(&mut self, i: &Actor, snapshot: Snapshot) -> Option<Writable<T>> {
        Some(Writable {
            actor: *i,
            snapshot,
            changes: &mut self.changes,
            data: self.storage.get_mut(i)?,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.storage.iter().map(|(k, v)| (*k, v))
    }

    pub fn remove(&mut self, i: &Actor, snapshot: Snapshot) -> Option<T> {
        self.changes.insert(*i, snapshot);
        self.storage.remove(i)
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

    pub fn insert(&mut self, i: Actor, new: T) -> Option<T> {
        self.storage.insert(i, new)
    }

    pub fn get(&self, i: &Actor) -> Option<&T> {
        self.storage.get(i)
    }

    pub fn get_mut(&mut self, i: &Actor) -> Option<&mut T> {
        self.storage.get_mut(i)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.storage.iter().map(|(&a, t)| (a, t))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Actor, &mut T)> {
        self.storage.iter_mut().map(|(&a, t)| (a, t))
    }

    pub fn remove(&mut self, i: &Actor) -> Option<T> {
        self.storage.remove(i)
    }
}
