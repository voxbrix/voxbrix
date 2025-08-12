use nohash_hasher::IntMap;
use serde::Deserialize;
use voxbrix_common::{
    entity::{
        actor::Actor,
        actor_class::ActorClass,
        update::Update,
    },
    messages::{
        ActorUpdateUnpack,
        UpdatesUnpacked,
    },
    pack,
    system::actor_class_loading::LoadActorClassComponent,
    AsFromUsize,
};

pub mod model;

/// Works as both Actor component and ActorClass component.
/// Actor component overrides component of its ActorClass.
/// Overrides are only meant to be coming from the server.
pub struct OverridableActorClassComponent<T> {
    update: Update,
    player_actor: Actor,
    is_client_controlled: bool,
    classes: Vec<Option<T>>,
    overrides: IntMap<Actor, T>,
}

impl<T> OverridableActorClassComponent<T> {
    pub fn new(update: Update, player_actor: Actor, is_client_controlled: bool) -> Self {
        Self {
            update,
            player_actor,
            is_client_controlled,
            classes: Vec::new(),
            overrides: IntMap::default(),
        }
    }

    pub fn get(&self, actor: &Actor, actor_class: &ActorClass) -> Option<&T> {
        self.overrides
            .get(actor)
            .or_else(|| self.classes.get(actor_class.as_usize())?.as_ref())
    }
}

impl<'a, T> OverridableActorClassComponent<T>
where
    T: Deserialize<'a>,
{
    pub fn unpack(&mut self, updates: &UpdatesUnpacked<'a>) {
        if let Some((changes, _)) = updates
            .get(&self.update)
            .and_then(|buffer| pack::decode_from_slice::<ActorUpdateUnpack<T>>(buffer))
        {
            match changes {
                ActorUpdateUnpack::Change(changes) => {
                    for (actor, change) in changes {
                        if let Some(component) = change {
                            self.overrides.insert(actor, component);
                        } else {
                            self.overrides.remove(&actor);
                        }
                    }
                },
                ActorUpdateUnpack::Full(full) => {
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
