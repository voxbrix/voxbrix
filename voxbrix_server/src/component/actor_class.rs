use crate::component::actor::ActorComponentPackable;
use anyhow::Error;
use nohash_hasher::IntSet;
use serde::Serialize;
use voxbrix_common::{
    component::StaticEntityComponent,
    entity::{
        actor::Actor,
        actor_class::ActorClass,
        snapshot::ServerSnapshot,
        update::Update,
    },
    messages::UpdatesPacker,
    FromDescriptor,
    LabelLibrary,
};
use voxbrix_world::{
    Initialization,
    World,
};

pub mod block_collision;
pub mod health;
pub mod hitbox;
pub mod model;

/// Works as both Actor update and ActorClass update.
/// Actor update overrides update of its ActorClass.
pub struct PackableOverridableActorClassComponent<T>
where
    T: 'static,
{
    classes: StaticEntityComponent<ActorClass, T>,
    overrides: ActorComponentPackable<T>,
}

impl<T> PackableOverridableActorClassComponent<T> {
    pub fn get(&self, class: &ActorClass, actor: &Actor) -> &T {
        self.overrides
            .get(actor)
            .unwrap_or_else(|| self.classes.get(class))
    }
}

impl<T> PackableOverridableActorClassComponent<T>
where
    T: PartialEq,
{
    /// Returns previous override for the actor, if any.
    pub fn insert(
        &mut self,
        class: &ActorClass,
        actor: &Actor,
        value: T,
        snapshot: ServerSnapshot,
    ) {
        let override_value = self.overrides.get(actor);
        let class_value = self.classes.get(class);

        if override_value != Some(&value) && class_value != &value {
            self.overrides.insert(*actor, value, snapshot);
        } else if override_value.is_some() && class_value == &value {
            self.overrides.remove(actor, snapshot);
        }
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

impl<T> Initialization for PackableOverridableActorClassComponent<T>
where
    T: FromDescriptor + WithUpdate + Default + Serialize + PartialEq + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let classes = StaticEntityComponent::initialization(world).await?;

        let update = world
            .get_resource_ref::<LabelLibrary>()
            .get::<Update>(T::UPDATE)
            .ok_or_else(|| anyhow::anyhow!("update with label \"{}\" is undefined", T::UPDATE))?;

        Ok(Self {
            classes,
            overrides: ActorComponentPackable::new(update),
        })
    }
}

pub trait WithUpdate {
    const UPDATE: &str;
}

impl<T> WithUpdate for Option<T>
where
    T: WithUpdate,
{
    const UPDATE: &str = T::UPDATE;
}
