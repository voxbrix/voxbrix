use nohash_hasher::IntMap;
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    collections::BTreeMap,
    ops::Deref,
};
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::Snapshot,
        state_component::StateComponent,
    },
    math::MinMax,
    messages::{
        ActorStateUnpack,
        State,
        StatePacker,
    },
    pack,
};

pub mod animation_state;
pub mod class;
pub mod orientation;
pub mod position;
pub mod velocity;

pub struct Writable<'a, T> {
    is_player: bool,
    snapshot: Snapshot,
    last_change_snapshot: &'a mut Snapshot,
    data: &'a mut T,
}

impl<'a, T> Writable<'a, T>
where
    T: PartialEq,
{
    /// Only updates value if it is different from the old one.
    pub fn update(&mut self, value: T) {
        let Self {
            snapshot,
            last_change_snapshot,
            is_player,
            data,
        } = self;

        if *is_player && value != **data {
            **data = value;
            **last_change_snapshot = *snapshot;
        }
    }
}

impl<'a, T> Deref for Writable<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.data
    }
}

/// Component that can be packed into State and sent to the server
pub struct ActorComponentPackable<T>
where
    T: 'static,
{
    state_component: StateComponent,
    is_client_controlled: bool,
    player_actor: Actor,
    last_change_snapshot: Snapshot,
    storage: IntMap<Actor, T>,
}

impl<T> ActorComponentPackable<T>
where
    T: 'static,
{
    pub fn new(
        state_component: StateComponent,
        player_actor: Actor,
        is_client_controlled: bool,
    ) -> Self {
        Self {
            state_component,
            player_actor,
            is_client_controlled,
            last_change_snapshot: Snapshot(0),
            storage: IntMap::default(),
        }
    }

    pub fn insert(&mut self, i: Actor, new: T, snapshot: Snapshot) -> Option<T> {
        self.last_change_snapshot = snapshot;
        self.storage.insert(i, new)
    }

    pub fn get(&self, i: &Actor) -> Option<&T> {
        self.storage.get(i)
    }

    pub fn get_writable(&mut self, i: &Actor, snapshot: Snapshot) -> Option<Writable<T>> {
        Some(Writable {
            is_player: *i == self.player_actor,
            snapshot,
            last_change_snapshot: &mut self.last_change_snapshot,
            data: self.storage.get_mut(i)?,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &T)> {
        self.storage.iter().map(|(k, v)| (*k, v))
    }

    pub fn remove(&mut self, i: &Actor, snapshot: Snapshot) -> Option<T> {
        self.last_change_snapshot = snapshot;
        self.storage.remove(i)
    }
}

impl<T> ActorComponentPackable<T>
where
    T: 'static + Serialize,
{
    pub fn pack_player(&mut self, state: &mut StatePacker, last_client_snapshot: Snapshot) {
        if last_client_snapshot < self.last_change_snapshot {
            let change = self.storage.get(&self.player_actor);

            let buffer = state.get_component_buffer(self.state_component);

            pack::serialize_into(&change, buffer);
        }
    }
}

impl<'a, T> ActorComponentPackable<T>
where
    T: Deserialize<'a>,
{
    pub fn unpack_state(&mut self, state: &State<'a>) {
        if let Some(changes) = state
            .get_component(&self.state_component)
            .and_then(|buffer| pack::deserialize_from::<ActorStateUnpack<T>>(buffer))
        {
            match changes {
                ActorStateUnpack::Change(changes) => {
                    for (actor, change) in changes {
                        if let Some(component) = change {
                            self.storage.insert(actor, component);
                        } else {
                            self.storage.remove(&actor);
                        }
                    }
                },
                ActorStateUnpack::Full(full) => {
                    let player_value = self.storage.remove(&self.player_actor);

                    self.storage.clear();
                    self.storage.extend(full.into_iter());

                    if let Some(player_value) = player_value {
                        if self.is_client_controlled {
                            self.storage.insert(self.player_actor, player_value);
                        }
                    }
                },
            }
        }
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

pub struct ActorSubcomponent<K, T> {
    data: BTreeMap<(Actor, K), T>,
}

impl<K, T> ActorSubcomponent<K, T>
where
    K: Ord + Copy + MinMax,
{
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, actor: Actor, key: K, new: T) -> Option<T> {
        self.data.insert((actor, key), new)
    }

    pub fn get(&self, actor: Actor, key: K) -> Option<&T> {
        self.data.get(&(actor, key))
    }

    pub fn get_actor(&self, actor: Actor) -> impl DoubleEndedIterator<Item = (Actor, K, &T)> {
        self.data
            .range((actor, K::MIN) .. (actor, K::MAX))
            .map(|(&(m, k), t)| (m, k, t))
    }

    pub fn extend(&mut self, iter: impl Iterator<Item = (Actor, K, T)>) {
        self.data.extend(iter.map(|(m, k, t)| ((m, k), t)))
    }

    pub fn remove(&mut self, actor: Actor, key: K) -> Option<T> {
        self.data.remove(&(actor, key))
    }
}
