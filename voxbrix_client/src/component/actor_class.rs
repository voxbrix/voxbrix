use bincode::BorrowDecode;
use nohash_hasher::IntMap;
use voxbrix_common::{
    entity::{
        actor::Actor,
        actor_class::ActorClass,
        state_component::StateComponent,
    },
    messages::{
        ActorStateUnpack,
        StateUnpacked,
    },
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
    is_client_controlled: bool,
    classes: Vec<Option<T>>,
    overrides: IntMap<Actor, T>,
}

impl<T> OverridableActorClassComponent<T> {
    pub fn new(
        state_component: StateComponent,
        player_actor: Actor,
        is_client_controlled: bool,
    ) -> Self {
        Self {
            state_component,
            player_actor,
            is_client_controlled,
            classes: Vec::new(),
            overrides: IntMap::default(),
        }
    }

    pub fn get(&self, actor: &Actor, actor_class: &ActorClass) -> Option<&T> {
        self.overrides
            .get(actor)
            .or_else(|| self.classes.get(actor_class.into_usize())?.as_ref())
    }

    pub fn remove_actor(&mut self, actor: &Actor) -> Option<T> {
        self.overrides.remove(actor)
    }
}

impl<'a, T> OverridableActorClassComponent<T>
where
    T: BorrowDecode<'a>,
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
                            self.overrides.insert(actor, component);
                        } else {
                            self.overrides.remove(&actor);
                        }
                    }
                },
                ActorStateUnpack::Full(full) => {
                    let player_value = self.overrides.remove(&self.player_actor);

                    self.overrides.clear();
                    self.overrides.extend(full.into_iter());

                    if let Some(player_value) = player_value {
                        if self.is_client_controlled {
                            self.overrides.insert(self.player_actor, player_value);
                        }
                    }
                },
            }
        }
    }
}

impl<T> LoadActorClassComponent<T> for OverridableActorClassComponent<T> {
    fn reload_classes(&mut self, data: Vec<Option<T>>) {
        self.classes = data;
    }
}
