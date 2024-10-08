use crate::component::actor::ActorComponentPackable;
use nohash_hasher::IntSet;
use serde::Serialize;
use voxbrix_common::{
    entity::{
        actor::Actor,
        snapshot::Snapshot,
        state_component::StateComponent,
    },
    messages::StatePacker,
    system::actor_class_loading::LoadActorClassComponent,
};

pub mod model;

/// Works as both Actor component and ActorClass component.
/// Actor component overrides component of its ActorClass.
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
    pub fn new(state_component: StateComponent) -> Self {
        Self {
            classes: Vec::new(),
            overrides: ActorComponentPackable::new(state_component),
        }
    }

    pub fn pack_full(
        &mut self,
        state: &mut StatePacker,
        player_actor: Option<&Actor>,
        actors_full_update: &IntSet<Actor>,
    ) {
        self.overrides
            .pack_full(state, player_actor, actors_full_update)
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
        self.overrides.pack_changes(
            state,
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
