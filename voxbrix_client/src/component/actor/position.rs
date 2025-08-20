use crate::component::actor::WritableTrait;
use nohash_hasher::IntMap;
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
        snapshot::ClientSnapshot,
        update::Update,
    },
    math::MinMax,
    messages::{
        ComponentUpdateUnpack,
        UpdatesPacker,
        UpdatesUnpacked,
    },
    pack,
};

pub struct Writable<'a> {
    is_player: bool,
    actor: Actor,
    snapshot: ClientSnapshot,
    last_change_snapshot: &'a mut ClientSnapshot,
    data: &'a mut Position,
    player_chunk_changes: &'a mut VecDeque<(ClientSnapshot, Chunk)>,
    chunk_actor_component: &'a mut BTreeSet<(Chunk, Actor)>,
}

impl<'a> WritableTrait<Position> for Writable<'a> {
    /// Only updates value if it is different from the old one.
    fn update(&mut self, value: Position) {
        let Self {
            snapshot,
            last_change_snapshot,
            is_player,
            actor,
            data,
            player_chunk_changes,
            chunk_actor_component,
        } = self;

        if *is_player && value != **data {
            **last_change_snapshot = *snapshot;
            if value.chunk != data.chunk {
                chunk_actor_component.remove(&(data.chunk, *actor));
                chunk_actor_component.insert((value.chunk, *actor));
                player_chunk_changes.push_back((*snapshot, value.chunk));
            }
        }

        **data = value;
    }
}

impl<'a> Deref for Writable<'a> {
    type Target = Position;

    fn deref(&self) -> &Position {
        self.data
    }
}

/// Component that can be packed into State and sent to the server.
/// Position is always client-controlled.
#[derive(Debug)]
pub struct PositionActorComponent {
    update: Update,
    player_actor: Actor,
    last_change_snapshot: ClientSnapshot,
    storage: IntMap<Actor, Position>,
    chunk_actor_component: BTreeSet<(Chunk, Actor)>,
    player_chunk_changes: VecDeque<(ClientSnapshot, Chunk)>,
}

impl PositionActorComponent {
    pub fn new(update: Update, player_actor: Actor) -> Self {
        Self {
            update,
            player_actor,
            last_change_snapshot: ClientSnapshot(0),
            storage: IntMap::default(),
            chunk_actor_component: BTreeSet::new(),
            player_chunk_changes: VecDeque::new(),
        }
    }

    pub fn insert(
        &mut self,
        actor: Actor,
        new: Position,
        snapshot: ClientSnapshot,
    ) -> Option<Position> {
        self.last_change_snapshot = snapshot;
        if actor == self.player_actor {
            self.player_chunk_changes.push_back((snapshot, new.chunk));
        }

        let prev_position = self.storage.insert(actor, new);

        if let Some(prev_position) = &prev_position {
            self.chunk_actor_component
                .remove(&(prev_position.chunk, actor));
            self.chunk_actor_component.insert((new.chunk, actor));
        }

        prev_position
    }

    pub fn get(&self, i: &Actor) -> Option<&Position> {
        self.storage.get(i)
    }

    pub fn get_writable(
        &mut self,
        actor: &Actor,
        snapshot: ClientSnapshot,
    ) -> Option<Writable<'_>> {
        Some(Writable {
            is_player: *actor == self.player_actor,
            actor: *actor,
            snapshot,
            last_change_snapshot: &mut self.last_change_snapshot,
            data: self.storage.get_mut(actor)?,
            player_chunk_changes: &mut self.player_chunk_changes,
            chunk_actor_component: &mut self.chunk_actor_component,
        })
    }

    pub fn iter(&self) -> impl Iterator<Item = (Actor, &Position)> {
        self.storage.iter().map(|(k, v)| (*k, v))
    }

    pub fn pack_player(
        &mut self,
        updates_packer: &mut UpdatesPacker,
        last_client_snapshot: ClientSnapshot,
    ) {
        if last_client_snapshot < self.last_change_snapshot {
            let change = self.storage.get(&self.player_actor);

            let buffer = updates_packer.get_buffer(self.update);

            pack::encode_into(&change, buffer);
        }
    }

    /// Must be done whenever the server confirms snapshot reception.
    pub fn confirm_snapshot(&mut self, curr_snapshot: ClientSnapshot) {
        while let Some((snapshot, _)) = self.player_chunk_changes.front() {
            if *snapshot > curr_snapshot || self.player_chunk_changes.len() <= 1 {
                break;
            }

            self.player_chunk_changes.pop_front();
        }
    }

    #[allow(dead_code)]
    pub fn get_actors_in_chunk(&self, chunk: &Chunk) -> impl Iterator<Item = Actor> + use<'_> {
        self.chunk_actor_component
            .range((*chunk, Actor::MIN) ..= (*chunk, Actor::MAX))
            .map(|(_, actor)| *actor)
    }

    /// Chunks that player belonged to for the last N unconfirmed snapshots.
    /// Order is "old -> new".
    pub fn player_chunks(&self) -> impl ExactSizeIterator<Item = &Chunk> {
        self.player_chunk_changes
            .iter()
            .map(|(_snapshot, chunk)| chunk)
    }

    pub fn remove(&mut self, actor: &Actor) -> Option<Position> {
        assert_ne!(
            *actor, self.player_actor,
            "removing position of the player actor is not supported"
        );

        let pos = self.storage.remove(actor);
        if let Some(pos) = &pos {
            self.chunk_actor_component.remove(&(pos.chunk, *actor));
        }

        pos
    }

    /// Special version of the "unpack" to sync state for interpolatable actor components,
    /// like orientation or position.
    /// Should be used together with the "target" version of the component - "target" uses
    /// [`unpack`] and the component itself uses [`unpack_target`].
    /// Internally does not directly set the component unless the change is a full update or
    /// a removal.
    pub fn unpack_target(&mut self, updates: &UpdatesUnpacked) {
        if let Some((changes, _)) = updates.get(&self.update).and_then(|buffer| {
            pack::decode_from_slice::<ComponentUpdateUnpack<Actor, Position>>(buffer)
        }) {
            match changes {
                ComponentUpdateUnpack::Change(changes) => {
                    for (actor, change) in changes {
                        if change.is_none() {
                            self.storage.remove(&actor);
                        }
                    }
                },
                ComponentUpdateUnpack::Full(full) => {
                    let player_value = self.storage.remove(&self.player_actor);

                    self.storage.clear();
                    self.storage.extend(full.into_iter());

                    if let Some(player_value) = player_value {
                        self.storage.insert(self.player_actor, player_value);
                    }
                },
            }
        }
    }
}
