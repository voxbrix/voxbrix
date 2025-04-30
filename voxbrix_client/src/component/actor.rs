use crate::system::movement_interpolation::TARGET_QUEUE_LENGTH;
use arrayvec::ArrayVec;
use nohash_hasher::IntMap;
use serde::{
    Deserialize,
    Serialize,
};
use std::{
    collections::{
        hash_map,
        BTreeMap,
    },
    ops::Deref,
    time::Instant,
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
        StatePacker,
        StateUnpacked,
    },
    pack,
};

pub mod animation_state;
pub mod class;
pub mod orientation;
pub mod position;
pub mod target_orientation;
pub mod target_position;
pub mod velocity;

pub trait WritableTrait<T>: Deref<Target = T> {
    /// Only updates value if it is different from the old one.
    fn update(&mut self, value: T);
}

pub struct Writable<'a, T> {
    is_player: bool,
    snapshot: Snapshot,
    last_change_snapshot: &'a mut Snapshot,
    data: &'a mut T,
}

impl<'a, T> WritableTrait<T> for Writable<'a, T>
where
    T: PartialEq,
{
    fn update(&mut self, value: T) {
        let Self {
            snapshot,
            last_change_snapshot,
            is_player,
            data,
        } = self;

        if *is_player && value != **data {
            **last_change_snapshot = *snapshot;
        }

        **data = value;
    }
}

impl<'a, T> Deref for Writable<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.data
    }
}

/// Component that can be packed into State and sent to the server.
/// Only the state of actor is packed and send, the rest of the actors
/// are unpacked from the server.
#[derive(Debug)]
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
    T: PartialEq + 'static,
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

    pub fn insert(&mut self, actor: Actor, new: T, snapshot: Snapshot) -> Option<T> {
        let entry = self.storage.entry(actor);

        match entry {
            hash_map::Entry::Occupied(mut prev) => {
                let changed = prev.get() == &new;

                if changed && actor == self.player_actor {
                    self.last_change_snapshot = snapshot;
                }

                Some(prev.insert(new))
            },
            hash_map::Entry::Vacant(slot) => {
                if actor == self.player_actor {
                    self.last_change_snapshot = snapshot;
                }

                slot.insert(new);

                None
            },
        }
    }

    pub fn get(&self, actor: &Actor) -> Option<&T> {
        self.storage.get(actor)
    }

    pub fn get_writable(&mut self, actor: &Actor, snapshot: Snapshot) -> Option<Writable<T>> {
        Some(Writable {
            is_player: *actor == self.player_actor,
            snapshot,
            last_change_snapshot: &mut self.last_change_snapshot,
            data: self.storage.get_mut(actor)?,
        })
    }

    pub fn remove(&mut self, actor: &Actor, snapshot: Snapshot) -> Option<T> {
        if self.player_actor == *actor {
            self.last_change_snapshot = snapshot;
        }
        self.storage.remove(actor)
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

            pack::encode_into(&change, buffer);
        }
    }
}

impl<'a, T> ActorComponentPackable<T>
where
    T: Deserialize<'a>,
{
    pub fn unpack_state(&mut self, state: &StateUnpacked<'a>) {
        if let Some((changes, _)) = state
            .get_component(&self.state_component)
            .and_then(|buffer| pack::decode_from_slice::<ActorStateUnpack<T>>(buffer))
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

    /// Special version of the "unpack_state" to sync state for interpolatable actor components,
    /// like orientation or position.
    /// Should be used together with the "target" version of the component - "target" uses
    /// [`unpack_state`] and the component itself uses [`unpack_state_target`].
    /// Internally does not directly set the component unless the change is a full update or
    /// a removal.
    pub fn unpack_state_target(&mut self, state: &StateUnpacked<'a>) {
        if let Some((changes, _)) = state
            .get_component(&self.state_component)
            .and_then(|buffer| pack::decode_from_slice::<ActorStateUnpack<T>>(buffer))
        {
            match changes {
                ActorStateUnpack::Change(changes) => {
                    for (actor, change) in changes {
                        if change.is_none() {
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

/// Internal component that is received from the server.
pub struct ActorComponentUnpackable<T> {
    state_component: StateComponent,
    storage: IntMap<Actor, T>,
}

impl<T> ActorComponentUnpackable<T> {
    pub fn new(state_component: StateComponent) -> Self {
        Self {
            state_component,
            storage: IntMap::default(),
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Actor, &mut T)> {
        self.storage.iter_mut().map(|(&a, t)| (a, t))
    }

    pub fn remove(&mut self, actor: &Actor) -> Option<T> {
        self.storage.remove(actor)
    }
}

impl<T> ActorComponentUnpackable<T> {
    pub fn unpack_state_convert<'a, U>(
        &mut self,
        state: &StateUnpacked<'a>,
        mut convert: impl FnMut(Actor, Option<T>, U) -> T,
    ) where
        U: Deserialize<'a>,
    {
        if let Some((changes, _)) = state
            .get_component(&self.state_component)
            .and_then(|buffer| pack::decode_from_slice::<ActorStateUnpack<U>>(buffer))
        {
            match changes {
                ActorStateUnpack::Change(changes) => {
                    for (actor, change) in changes {
                        if let Some(component) = change {
                            let previous = self.storage.remove(&actor);
                            self.storage
                                .insert(actor, convert(actor, previous, component));
                        } else {
                            self.storage.remove(&actor);
                        }
                    }
                },
                ActorStateUnpack::Full(full) => {
                    let full = full
                        .into_iter()
                        .map(|(actor, component)| {
                            let previous = self.storage.remove(&actor);
                            (actor, convert(actor, previous, component))
                        })
                        .collect::<Vec<_>>();

                    self.storage.clear();
                    self.storage.extend(full);
                },
            }
        }
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

    pub fn get(&self, actor: &Actor, key: &K) -> Option<&T> {
        self.data.get(&(*actor, *key))
    }

    // pub fn get_actor(&self, actor: &Actor) -> impl DoubleEndedIterator<Item = (Actor, K, &T)> {
    // self.data
    // .range((*actor, K::MIN) .. (*actor, K::MAX))
    // .map(|(&(m, k), t)| (m, k, t))
    // }

    pub fn remove(&mut self, actor: &Actor, key: &K) -> Option<T> {
        self.data.remove(&(*actor, *key))
    }

    pub fn remove_actor(&mut self, actor: &Actor) -> BTreeMap<(Actor, K), T> {
        let mut removed = self.data.split_off(&(*actor, K::MIN));

        let mut right_part = removed.split_off(&(*actor, K::MAX));

        self.data.append(&mut right_part);

        removed
    }
}

const TARGET_QUEUE_LENGTH_EXTRA: usize = TARGET_QUEUE_LENGTH + 1;

#[derive(Clone, Copy)]
pub struct Target<T> {
    pub server_snapshot: Snapshot,
    pub value: T,
    pub reach_time: Instant,
}

pub struct TargetQueue<T> {
    pub starting: T,
    pub target_queue: ArrayVec<Target<T>, TARGET_QUEUE_LENGTH_EXTRA>,
}

impl<T> TargetQueue<T> {
    pub fn from_previous(
        previous: Option<TargetQueue<T>>,
        current_value: T,
        new_target: T,
        current_time: Instant,
        server_snapshot: Snapshot,
    ) -> Self
    where
        T: Copy,
    {
        let mut target_queue = previous.unwrap_or(TargetQueue {
            starting: current_value,
            target_queue: ArrayVec::new(),
        });

        let last = target_queue.target_queue.last().copied();

        if last.is_none() || last.is_some() && last.unwrap().server_snapshot < server_snapshot {
            let new = Target {
                server_snapshot,
                value: new_target,
                reach_time: current_time,
            };

            if target_queue.target_queue.is_full() {
                *target_queue.target_queue.last_mut().unwrap() = new;
            } else {
                target_queue.target_queue.push(new);
            }
        }

        target_queue
    }
}
