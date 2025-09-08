use anyhow::Error;
use nohash_hasher::IntMap;
use serde::Deserialize;
use voxbrix_common::{
    entity::{
        actor::Actor,
        actor_class::ActorClass,
        update::Update,
    },
    messages::{
        ComponentUpdateUnpack,
        UpdatesUnpacked,
    },
    pack,
    system::component_map::ComponentMap,
    AsFromUsize,
    LabelLibrary,
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
    pub fn new<'de, 'label, D>(
        update: Update,
        player_actor: Actor,
        is_client_controlled: bool,
        component_map: &'de ComponentMap<ActorClass>,
        label_library: &LabelLibrary,
        component_name: &'label str,
        convert: impl Fn(D) -> Result<T, Error>,
    ) -> Result<Self, Error>
    where
        D: Deserialize<'de>,
        'label: 'de,
    {
        let mut vec = Vec::new();

        vec.resize_with(
            label_library
                .get_label_map_for::<ActorClass>()
                .expect("ActorClass label map is undefined")
                .len(),
            || None,
        );

        for res in component_map.get_component::<'de, 'label, D>(component_name) {
            let (e, d) = res?;

            vec[e.as_usize()] = Some(convert(d)?);
        }

        Ok(Self {
            update,
            player_actor,
            is_client_controlled,
            classes: vec,
            overrides: IntMap::default(),
        })
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
            .and_then(|buffer| pack::decode_from_slice::<ComponentUpdateUnpack<Actor, T>>(buffer))
        {
            match changes {
                ComponentUpdateUnpack::Change(changes) => {
                    for (actor, change) in changes {
                        if let Some(component) = change {
                            self.overrides.insert(actor, component);
                        } else {
                            self.overrides.remove(&actor);
                        }
                    }
                },
                ComponentUpdateUnpack::Full(full) => {
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
