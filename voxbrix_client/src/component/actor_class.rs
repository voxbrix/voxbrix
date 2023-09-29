use nohash_hasher::IntMap;
use serde::Deserialize;
use voxbrix_common::{
    entity::{
        actor::Actor,
        actor_class::ActorClass,
        state_component::StateComponent,
    },
    messages::State,
    pack,
    system::actor_class_loading::LoadActorClassComponent,
};

pub mod model;

/// Works as both Actor component and ActorClass component.
/// Actor component overrides component of its ActorClass.
/// Overrides are only meant to be coming from the server.
pub struct OverridableActorClassComponent<T> {
    state_component: StateComponent,
    player_actor: Actor,
    classes: Vec<Option<T>>,
    overrides: IntMap<Actor, T>,
}

impl<T> OverridableActorClassComponent<T> {
    pub fn new(state_component: StateComponent, player_actor: Actor) -> Self {
        Self {
            state_component,
            player_actor,
            classes: Vec::new(),
            overrides: IntMap::default(),
        }
    }

    pub fn get(&self, actor: &Actor, actor_class: &ActorClass) -> Option<&T> {
        self.overrides
            .get(actor)
            .or_else(|| self.classes.get(actor_class.0)?.as_ref())
    }

    pub fn remove_actor(&mut self, actor: &Actor) -> Option<T> {
        self.overrides.remove(actor)
    }
}

impl<'a, T> OverridableActorClassComponent<T>
where
    T: Deserialize<'a>,
{
    pub fn unpack_state(&mut self, state: &State<'a>) {
        if let Some(changes) = state
            .get_component(&self.state_component)
            .and_then(|buffer| pack::deserialize_from::<Vec<(Actor, Option<T>)>>(buffer))
        {
            for (actor, change) in changes {
                if let Some(component) = change {
                    self.overrides.insert(actor, component);
                } else {
                    self.overrides.remove(&actor);
                }
            }
        }
    }
}

impl<T> LoadActorClassComponent<T> for OverridableActorClassComponent<T> {
    fn reload_classes(&mut self, data: Vec<Option<T>>) {
        self.classes = data;
    }
}
