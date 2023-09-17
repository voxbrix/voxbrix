use crate::component::actor::ActorComponentPacker;
use nohash_hasher::{
    IntMap,
    IntSet,
};
use std::{
    collections::{
        BTreeSet,
        VecDeque,
    },
    ops::Deref,
};
use voxbrix_common::{
    component::actor::position::Position,
    entity::{
        actor::Actor,
        chunk::Chunk,
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

pub struct Writable<'a, T> {
    actor: Actor,
    snapshot: Snapshot,
    changes: &'a mut IntMap<Actor, Snapshot>,
    chunk_changes: &'a mut VecDeque<ActorChunkChange>,
    chunk_actor_component: &'a mut BTreeSet<(Chunk, Actor)>,
    data: &'a mut T,
}

impl Writable<'_, Position> {
    /// Only updates value if it is different from the old one.
    pub fn update(&mut self, value: Position) {
        let Self {
            snapshot,
            changes,
            chunk_changes,
            chunk_actor_component,
            actor,
            data,
        } = self;

        if value != **data {
            if value.chunk != data.chunk {
                chunk_changes.push_back(ActorChunkChange {
                    snapshot: *snapshot,
                    actor: *actor,
                    previous_chunk: Some(data.chunk),
                });

                chunk_actor_component.remove(&(data.chunk, *actor));
                chunk_actor_component.insert((value.chunk, *actor));
            }

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

struct ActorChunkChange {
    snapshot: Snapshot,
    actor: Actor,
    previous_chunk: Option<Chunk>,
}

/// Special container for the position component.
/// There are some specifics related to position being main factor to
/// determine if and how (complete/change-only) the Actor should be sent to the client.
pub struct PositionActorComponent {
    state_component: StateComponent,
    last_packed_snapshot: Snapshot,
    changes: IntMap<Actor, Snapshot>,
    chunk_changes: VecDeque<ActorChunkChange>,
    packer: Option<ActorComponentPacker<'static, Position>>,
    storage: IntMap<Actor, Position>,
    chunk_actor_component: BTreeSet<(Chunk, Actor)>,
    actors_full_update: IntSet<Actor>,
}

impl PositionActorComponent {
    /// First argument of the `func` is the old value, second - new one
    pub fn unpack_player_with<U>(
        &mut self,
        player_actor: &Actor,
        state: &State,
        snapshot: Snapshot,
        mut func: impl FnMut(Option<&Position>, Option<&Position>) -> U,
    ) -> U {
        let prev_value = self.storage.get(player_actor);

        if let Some(change) = state
            .get_component(&self.state_component)
            .and_then(|buf| pack::deserialize_from::<Option<Position>>(buf))
        {
            let result = func(prev_value, change.as_ref());

            if let Some(new_value) = change {
                self.insert(*player_actor, new_value, snapshot);
            } else {
                self.remove(player_actor, snapshot);
            }

            result
        } else {
            func(prev_value, None)
        }
    }
}

impl PositionActorComponent {
    pub fn new(state_component: StateComponent) -> Self {
        Self {
            state_component,
            last_packed_snapshot: Snapshot(0),
            changes: IntMap::default(),
            chunk_changes: VecDeque::new(),
            packer: Some(ActorComponentPacker::new()),
            storage: IntMap::default(),
            chunk_actor_component: BTreeSet::new(),
            actors_full_update: IntSet::default(),
        }
    }

    /// Returns the Vec of Actors that have to have all components packed.
    pub fn pack_changes(
        &mut self,
        state: &mut StatePacker,
        snapshot: Snapshot,
        last_server_snapshot: Snapshot,
        player_actor: &Actor,
        // Checks should include:
        //     The Actor is within the territory that persists within player's view field
        //         while player moves from the old position (during last_server_snapshot)
        //         to the current (during snapshot) one.
        //         None argument considered to be "out", so must return `false`.
        is_within_intersection: impl Fn(Option<&Chunk>) -> bool,
        // Those will have to have all components packed:
        add_actors_in_chunks: impl Iterator<Item = Chunk>,
    ) {
        if snapshot.0 > self.last_packed_snapshot.0 {
            self.changes.retain(move |_, change_snapshot| {
                snapshot.0 - change_snapshot.0 <= MAX_SNAPSHOT_DIFF
            });

            while self.chunk_changes.front().is_some()
                && snapshot.0 - self.chunk_changes.front().unwrap().snapshot.0 > MAX_SNAPSHOT_DIFF
            {
                self.chunk_changes.pop_front();
            }

            self.last_packed_snapshot = snapshot;
        }

        self.actors_full_update.clear();
        self.actors_full_update.extend(
            add_actors_in_chunks
                .flat_map(|chunk| {
                    // Actors on chunks that were freshly loaded.
                    // TODO?: currently includes Actors that were already loaded in the intersection
                    // of the old chunks and new chunks (persisted chunks) and move simultaniously with
                    // the player to the new (unloaded before) chunks loaded by the player.
                    // This could lead to sending redundant data about these Actors, but this
                    // is a complex edge case that (with removing moved-away-from-view actors in client) could
                    // introduce subtle but serious bugs if we exclude those Actors.
                    self.chunk_actor_component
                        .range((chunk, Actor(0)) ..= (chunk, Actor(usize::MAX)))
                        .map(|(_, actor)| (*actor))
                })
                .chain(
                    self.chunk_changes
                        .iter()
                        .filter(
                            |ActorChunkChange {
                                 snapshot,
                                 actor,
                                 previous_chunk,
                             }| {
                                // Actors that moved into the intersection (persisted chunks).
                                //
                                if last_server_snapshot >= *snapshot {
                                    // The player already knows about those changes.
                                    return false;
                                }

                                if is_within_intersection(previous_chunk.as_ref()) {
                                    // We only consider Actor moved in.
                                    return false;
                                }

                                // Current Actor's chunk:
                                is_within_intersection(
                                    self.storage.get(&actor).map(|pos| pos.chunk).as_ref(),
                                )
                            },
                        )
                        .map(|c| c.actor),
                )
                .filter(|actor| actor != player_actor || last_server_snapshot == Snapshot(0)),
        );

        // Actors that moved out of the intersection (persisted chunks).
        let actors_moved_away = self
            .chunk_changes
            .iter()
            .filter(
                |ActorChunkChange {
                     snapshot,
                     actor,
                     previous_chunk,
                 }| {
                    if last_server_snapshot >= *snapshot {
                        // The player already knows about those changes.
                        return false;
                    }

                    if !is_within_intersection(previous_chunk.as_ref()) {
                        // We only consider Actor moved out.
                        return false;
                    }

                    // Current Actor's chunk:
                    !is_within_intersection(self.storage.get(&actor).map(|pos| pos.chunk).as_ref())
                },
            )
            .map(|c| (c.actor, self.storage.get(&c.actor)));

        let change_iter = self
            .changes
            .iter()
            .filter(move |(_, change_snapshot)| change_snapshot.0 > last_server_snapshot.0)
            .map(|(actor, _)| (*actor, self.storage.get(actor)))
            .filter(|(_, value)| is_within_intersection(value.map(|v| v.chunk).as_ref()))
            .chain(actors_moved_away)
            .filter(|(actor, _)| actor != player_actor)
            // Avoiding duplication:
            .filter(|(actor, _)| !self.actors_full_update.contains(actor))
            // The above check is already included for full-packed actors:
            .chain(
                self.actors_full_update
                    .iter()
                    .map(|actor| (*actor, self.storage.get(actor))),
            );

        let mut packer = self.packer.take().unwrap();

        let buffer = state.get_component_buffer(self.state_component);

        packer = packer.load(change_iter).pack(buffer);

        self.packer = Some(packer);
    }

    pub fn actors_full_update(&self) -> &IntSet<Actor> {
        &self.actors_full_update
    }

    pub fn insert(&mut self, actor: Actor, value: Position, snapshot: Snapshot) {
        let prev_value = self.storage.insert(actor, value);
        let value = self.storage.get(&actor).unwrap();

        let (changed, chunk_changed, previous_chunk) = match prev_value {
            Some(prev_value) => {
                (
                    &prev_value != value,
                    prev_value.chunk != value.chunk,
                    Some(prev_value.chunk),
                )
            },
            None => (true, true, None),
        };

        if changed {
            self.changes.insert(actor, snapshot);
            if chunk_changed {
                self.chunk_changes.push_back(ActorChunkChange {
                    snapshot,
                    actor,
                    previous_chunk,
                });

                self.chunk_actor_component.insert((value.chunk, actor));
            }
        }
    }

    pub fn get(&self, i: &Actor) -> Option<&Position> {
        self.storage.get(i)
    }

    pub fn get_writable(&mut self, i: &Actor, snapshot: Snapshot) -> Option<Writable<Position>> {
        Some(Writable {
            actor: *i,
            snapshot,
            changes: &mut self.changes,
            chunk_changes: &mut self.chunk_changes,
            chunk_actor_component: &mut self.chunk_actor_component,
            data: self.storage.get_mut(i)?,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &Position)> {
        self.storage.iter().map(|(k, v)| (*k, v))
    }

    pub fn remove(&mut self, actor: &Actor, snapshot: Snapshot) {
        if let Some(value) = self.storage.remove(actor) {
            self.chunk_changes.push_back(ActorChunkChange {
                snapshot,
                actor: *actor,
                previous_chunk: Some(value.chunk),
            });

            self.chunk_actor_component.remove(&(value.chunk, *actor));

            self.changes.insert(*actor, snapshot);
        }
    }
}
