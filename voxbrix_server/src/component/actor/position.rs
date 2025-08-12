use crate::component::actor::ActorComponentPacker;
use nohash_hasher::{
    IntMap,
    IntSet,
};
use rayon::prelude::*;
use std::{
    collections::{
        hash_map,
        BTreeSet,
        VecDeque,
    },
    ops::Deref,
};
use voxbrix_common::{
    component::actor::{
        position::Position,
        velocity::Velocity,
    },
    entity::{
        actor::Actor,
        chunk::Chunk,
        snapshot::{
            ServerSnapshot,
            MAX_SNAPSHOT_DIFF,
        },
        update::Update,
    },
    math::MinMax,
    messages::{
        UpdatesPacker,
        UpdatesUnpacked,
    },
    pack,
};

pub struct Change {
    pub actor: Actor,
    #[allow(dead_code)]
    pub prev_position: Position,
    pub next_position: Position,
    #[allow(dead_code)]
    pub prev_velocity: Velocity,
    pub next_velocity: Velocity,
    pub collides_with_block: bool,
}

pub struct PositionChanges(Vec<Change>);

impl PositionChanges {
    pub fn new() -> PositionChanges {
        PositionChanges(Vec::new())
    }

    pub fn from_par_iter(&mut self, iter: impl ParallelIterator<Item = Change>) {
        self.0.clear();
        self.0.par_extend(iter);
    }
}

impl Deref for PositionChanges {
    type Target = [Change];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

// pub struct Writable<'a, T> {
// actor: Actor,
// snapshot: Snapshot,
// changes: &'a mut IntMap<Actor, Snapshot>,
// chunk_changes: &'a mut VecDeque<ActorChunkChange>,
// chunk_actor_component: &'a mut BTreeSet<(Chunk, Actor)>,
// data: &'a mut T,
// }
//
// impl Writable<'_, Position> {
// Only updates value if it is different from the old one.
// pub fn update(&mut self, value: Position) {
// let Self {
// snapshot,
// changes,
// chunk_changes,
// chunk_actor_component,
// actor,
// data,
// } = self;
//
// if value != **data {
// if value.chunk != data.chunk {
// chunk_changes.push_back(ActorChunkChange {
// snapshot: *snapshot,
// actor: *actor,
// previous_chunk: Some(data.chunk),
// });
//
// chunk_actor_component.remove(&(data.chunk, *actor));
// chunk_actor_component.insert((value.chunk, *actor));
// }
//
// data = value;
//
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

pub struct ActorChunkChange {
    pub snapshot: ServerSnapshot,
    pub actor: Actor,
    pub previous_chunk: Option<Chunk>,
}

/// Special container for the position component.
/// There are some specifics related to position being main factor to
/// determine if and how (complete/change-only) the Actor should be sent to the client.
pub struct PositionActorComponent {
    update: Update,
    last_packed_snapshot: ServerSnapshot,
    changes: IntMap<Actor, ServerSnapshot>,
    chunk_changes: VecDeque<ActorChunkChange>,
    packer: Option<ActorComponentPacker<'static, Position>>,
    storage: IntMap<Actor, Position>,
    chunk_actor_component: BTreeSet<(Chunk, Actor)>,
    /// Actors that must have all components packed.
    /// Filled on packing this component.
    /// Includes the Player Actor.
    actors_full_update: IntSet<Actor>,
    /// Actors that can have changes in components.
    /// Filled on packing this component.
    /// Includes the Player Actor.
    actors_partial_update: IntSet<Actor>,
}

impl PositionActorComponent {
    /// First argument of the `func` is the old value, second - new one
    pub fn unpack_player_with<U>(
        &mut self,
        player_actor: &Actor,
        updates: &UpdatesUnpacked,
        snapshot: ServerSnapshot,
        mut func: impl FnMut(Option<&Position>, Option<&Position>) -> U,
    ) -> U {
        let prev_value = self.storage.get(player_actor);

        if let Some(change) = updates
            .get(&self.update)
            .and_then(|buf| pack::decode_from_slice::<Option<Position>>(buf))
            .map(|tup| tup.0)
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
    pub fn new(update: Update) -> Self {
        Self {
            update,
            last_packed_snapshot: ServerSnapshot(0),
            changes: IntMap::default(),
            chunk_changes: VecDeque::new(),
            packer: Some(ActorComponentPacker::new()),
            storage: IntMap::default(),
            chunk_actor_component: BTreeSet::new(),
            actors_full_update: IntSet::default(),
            actors_partial_update: IntSet::default(),
        }
    }

    /// Packs all the data for the actors in the chunks.
    /// Saves the list of actors, so other components could do the same.
    pub fn pack_full(
        &mut self,
        updates: &mut UpdatesPacker,
        player_actor: &Actor,
        // Those will have to have all components packed:
        full_update_chunks: impl Iterator<Item = Chunk>,
    ) {
        self.actors_full_update.clear();
        self.actors_partial_update.clear();

        self.actors_full_update
            .extend(full_update_chunks.flat_map(|chunk| {
                // Actors on chunks that were freshly loaded.
                // TODO?: currently includes Actors that were already loaded in the intersection
                // of the old chunks and new chunks (persisted chunks) and move simultaniously with
                // the player to the new (unloaded before) chunks loaded by the player.
                // This could lead to sending redundant data about these Actors, but this
                // is a complex edge case that (with removing moved-away-from-view actors in client) could
                // introduce subtle but serious bugs if we exclude those Actors.
                self.chunk_actor_component
                    .range((chunk, Actor::MIN) ..= (chunk, Actor::MAX))
                    .map(|(_, actor)| (*actor))
            }));

        let change_iter = self
            .actors_full_update
            .iter()
            .filter(|actor| *actor != player_actor)
            .filter_map(|actor| Some((*actor, self.storage.get(actor)?)));

        let mut packer = self.packer.take().unwrap();

        let buffer = updates.get_buffer(self.update);

        packer = packer.load_full(change_iter).pack(buffer);

        self.packer = Some(packer);
    }

    pub fn pack_changes(
        &mut self,
        updates: &mut UpdatesPacker,
        snapshot: ServerSnapshot,
        last_server_snapshot: ServerSnapshot,
        player_actor: &Actor,
        // Checks should include:
        //     The Actor is within the territory that persists within player's view field
        //         while player moves from the old position (during last_server_snapshot)
        //         to the current (during snapshot) one.
        //         None argument considered to be "out", so must return `false`.
        is_within_intersection: impl Fn(Option<&Chunk>) -> bool,
        // Those will have to have all components packed (new chunks):
        full_update_chunks: impl Iterator<Item = Chunk>,
        // Those must have changes packed (new/old intersection chunks):
        partial_update_chunks: impl Iterator<Item = Chunk>,
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
        self.actors_partial_update.clear();

        self.actors_full_update.extend(
            full_update_chunks
                .flat_map(|chunk| {
                    // Actors on chunks that were freshly loaded.
                    // TODO?: currently includes Actors that were already loaded in the intersection
                    // of the old chunks and new chunks (persisted chunks) and move simultaniously with
                    // the player to the new (unloaded before) chunks loaded by the player.
                    // This could lead to sending redundant data about these Actors, but this
                    // is a complex edge case that (with removing moved-away-from-view actors in client) could
                    // introduce subtle but serious bugs if we exclude those Actors.
                    self.chunk_actor_component
                        .range((chunk, Actor::MIN) ..= (chunk, Actor::MAX))
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
                ),
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
            .map(|c| c.actor);

        self.actors_partial_update.extend(
            partial_update_chunks
                .flat_map(|chunk| {
                    self.chunk_actor_component
                        .range((chunk, Actor::MIN) ..= (chunk, Actor::MAX))
                        .map(|(_, actor)| (*actor))
                })
                .chain(actors_moved_away)
                .filter(|actor| !self.actors_full_update.contains(actor)),
        );

        let change_iter = self
            .actors_partial_update
            .iter()
            .filter_map(|actor| self.changes.get_key_value(&actor))
            .filter(move |(_, change_snapshot)| change_snapshot.0 > last_server_snapshot.0)
            .map(|(actor, _)| actor)
            .chain(self.actors_full_update.iter())
            .filter(|actor| *actor != player_actor)
            .map(|actor| (*actor, self.storage.get(actor)));

        let mut packer = self.packer.take().unwrap();

        let buffer = updates.get_buffer(self.update);

        packer = packer.load_changes(change_iter).pack(buffer);

        self.packer = Some(packer);
    }

    /// Filled on packing this component.
    /// Includes the Player Actor.
    pub fn actors_partial_update(&self) -> &IntSet<Actor> {
        &self.actors_partial_update
    }

    /// Filled on packing this component.
    /// Includes the Player Actor.
    pub fn actors_full_update(&self) -> &IntSet<Actor> {
        &self.actors_full_update
    }

    /// Changed of chunks by actors, in "old snapshot to new snapshot" order.
    pub fn actors_chunk_changes(&self) -> impl DoubleEndedIterator<Item = &ActorChunkChange> {
        self.chunk_changes.iter()
    }

    pub fn insert(&mut self, actor: Actor, value: Position, snapshot: ServerSnapshot) {
        let (changed, chunk_changed, previous_chunk) = match self.storage.entry(actor) {
            hash_map::Entry::Occupied(mut slot) => {
                let prev_value = slot.insert(value);
                let value = slot.get();

                (
                    &prev_value != value,
                    prev_value.chunk != value.chunk,
                    Some(prev_value.chunk),
                )
            },
            hash_map::Entry::Vacant(slot) => {
                slot.insert(value);
                (true, true, None)
            },
        };

        if changed {
            self.changes.insert(actor, snapshot);
            if chunk_changed {
                self.chunk_changes.push_back(ActorChunkChange {
                    snapshot,
                    actor,
                    previous_chunk,
                });

                if let Some(previous_chunk) = previous_chunk {
                    self.chunk_actor_component.remove(&(previous_chunk, actor));
                }
                self.chunk_actor_component.insert((value.chunk, actor));
            }
        }
    }

    pub fn get(&self, i: &Actor) -> Option<&Position> {
        self.storage.get(i)
    }

    // pub fn get_writable(&mut self, i: &Actor, snapshot: Snapshot) -> Option<Writable<Position>> {
    // Some(Writable {
    // actor: *i,
    // snapshot,
    // changes: &mut self.changes,
    // chunk_changes: &mut self.chunk_changes,
    // chunk_actor_component: &mut self.chunk_actor_component,
    // data: self.storage.get_mut(i)?,
    // })
    // }

    pub fn remove(&mut self, actor: &Actor, snapshot: ServerSnapshot) {
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

    pub fn get_actors_in_chunk<'a>(&'a self, chunk: Chunk) -> impl Iterator<Item = Actor> + 'a {
        self.chunk_actor_component
            .range((chunk, Actor::MIN) ..= (chunk, Actor::MAX))
            .map(|(_, actor)| (*actor))
    }
}
