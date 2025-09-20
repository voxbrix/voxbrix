use crate::component::actor::ActorComponentPackable;
use anyhow::Error;
use nohash_hasher::IntSet;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_common::{
    entity::{
        actor::Actor,
        actor_class::ActorClass,
        snapshot::ServerSnapshot,
        update::Update,
    },
    messages::UpdatesPacker,
    system::component_map::ComponentMap,
    AsFromUsize,
    LabelLibrary,
};

pub mod block_collision;
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
    pub fn new<'de, 'label, D>(
        component_map: &'de ComponentMap<ActorClass>,
        label_library: &LabelLibrary,
        update: Update,
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
            classes: vec,
            overrides: ActorComponentPackable::new(update),
        })
    }
}

impl<T> PackableOverridableActorClassComponent<T> {
    #[allow(dead_code)]
    pub fn get(&self, class: &ActorClass, actor: &Actor) -> Option<&T> {
        self.overrides.get(actor).or_else(|| {
            self.classes
                .get(class.as_usize())
                .map(|o| o.as_ref())
                .flatten()
        })
    }
}

impl<T> PackableOverridableActorClassComponent<T>
where
    T: 'static + Serialize + PartialEq,
{
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
