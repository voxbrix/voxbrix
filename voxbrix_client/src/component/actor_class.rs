use crate::resource::player_actor::PlayerActor;
use anyhow::Error;
use nohash_hasher::IntMap;
use serde::Deserialize;
use voxbrix_common::{
    component::StaticEntityComponent,
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
    FromDescriptor,
    LabelLibrary,
};
use voxbrix_world::{
    Initialization,
    World,
};

pub mod block_collision;
pub mod health;
pub mod model;

/// Works as both Actor component and ActorClass component.
/// Actor component overrides component of its ActorClass.
/// Overrides are only meant to be coming from the server.
pub struct OverridableActorClassComponent<T> {
    update: Update,
    player_actor: Actor,
    is_client_controlled: bool,
    classes: StaticEntityComponent<ActorClass, T>,
    overrides: IntMap<Actor, T>,
}

impl<T> OverridableActorClassComponent<T> {
    pub fn get(&self, actor_class: &ActorClass, actor: &Actor) -> &T {
        self.overrides
            .get(actor)
            .unwrap_or_else(|| self.classes.get(actor_class))
    }
}

pub trait OverridableFromDescriptor: FromDescriptor {
    const UPDATE_LABEL: &str;
    const IS_CLIENT_CONTROLLED: bool;
}

impl<T> OverridableFromDescriptor for Option<T>
where
    T: OverridableFromDescriptor,
{
    const IS_CLIENT_CONTROLLED: bool = T::IS_CLIENT_CONTROLLED;
    const UPDATE_LABEL: &str = T::UPDATE_LABEL;
}

impl<T> Initialization for OverridableActorClassComponent<T>
where
    T: OverridableFromDescriptor + Default + Send + Sync + 'static,
{
    type Error = Error;

    async fn initialization(world: &World) -> Result<Self, Self::Error> {
        let classes = StaticEntityComponent::initialization(world).await?;

        let update = world
            .get_resource_ref::<LabelLibrary>()
            .get(T::UPDATE_LABEL)
            .ok_or_else(|| {
                anyhow::anyhow!("update with label \"{}\" is undefined", T::UPDATE_LABEL)
            })?;

        let player_actor = world.get_resource_ref::<PlayerActor>().0;

        Ok(Self {
            update,
            player_actor,
            is_client_controlled: T::IS_CLIENT_CONTROLLED,
            classes,
            overrides: IntMap::default(),
        })
    }
}

impl<'a, T> OverridableActorClassComponent<T>
where
    T: Deserialize<'a>,
{
    pub fn unpack(&mut self, updates: &UpdatesUnpacked<'a>) {
        if let Some((changes, _)) = updates
            .get(&self.update)
            .and_then(pack::decode_from_slice::<ComponentUpdateUnpack<Actor, T>>)
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
                    self.overrides.extend(full);

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
