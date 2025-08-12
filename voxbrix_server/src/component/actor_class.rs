use crate::component::actor::ActorComponentPackable;
use nohash_hasher::IntSet;
use serde::Serialize;
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::ServerSnapshot,
        update::Update,
    },
    messages::UpdatesPacker,
    system::actor_class_loading::LoadActorClassComponent,
};

pub mod model;

/// Works as both Actor update and ActorClass update.
/// Actor update overrides update of its ActorClass.
pub struct PackableOverridableActorClassComponent<T>
where
    T: 'static,
{
    classes: Vec<Option<T>>,
    overrides: ActorComponentPackable<T>,
}

impl<T> PackableOverridableActorClassComponent<T>
where
    T: 'static + Serialize + PartialEq,
{
    pub fn new(update: Update) -> Self {
        Self {
            classes: Vec::new(),
            overrides: ActorComponentPackable::new(update),
        }
    }

    pub fn pack_full(
        &mut self,
        updates: &mut UpdatesPacker,
        player_actor: Option<&Actor>,
        actors_full_update: &IntSet<Actor>,
    ) {
        self.overrides
            .pack_full(updates, player_actor, actors_full_update)
    }

    pub fn pack_changes(
        &mut self,
        updates: &mut UpdatesPacker,
        snapshot: ServerSnapshot,
        client_last_snapshot: ServerSnapshot,
        player_actor: Option<&Actor>,
        actors_full_update: &IntSet<Actor>,
        actors_partial_update: &IntSet<Actor>,
    ) {
        self.overrides.pack_changes(
            updates,
            snapshot,
            client_last_snapshot,
            player_actor,
            actors_full_update,
            actors_partial_update,
        )
    }
}

impl<T> LoadActorClassComponent<T> for PackableOverridableActorClassComponent<T> {
    fn reload_classes(&mut self, data: Vec<Option<T>>) {
        self.classes = data;
    }
}
