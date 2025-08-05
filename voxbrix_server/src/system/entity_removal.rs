use crate::{
    component::{
        actor::{
            chunk_activation::ChunkActivationActorComponent,
            class::ClassActorComponent,
            orientation::OrientationActorComponent,
            player::PlayerActorComponent,
            position::PositionActorComponent,
            projectile::ProjectileActorComponent,
            velocity::VelocityActorComponent,
        },
        player::{
            actions_packer::ActionsPackerPlayerComponent,
            actor::ActorPlayerComponent,
            chunk_update::ChunkUpdatePlayerComponent,
            chunk_view::ChunkViewPlayerComponent,
            client::ClientPlayerComponent,
        },
    },
    entity::{
        actor::ActorRegistry,
        player::Player,
    },
};
use voxbrix_common::{
    component::actor::effect::EffectActorComponent,
    entity::{
        actor::Actor,
        snapshot::ServerSnapshot,
    },
    resource::removal_queue::RemovalQueue,
};
use voxbrix_world::{
    System,
    SystemData,
};

pub struct EntityRemovalCheckSystem;

impl System for EntityRemovalCheckSystem {
    type Data<'a> = EntityRemovalCheckSystemData<'a>;
}

#[derive(SystemData)]
pub struct EntityRemovalCheckSystemData<'a> {
    actor_rq: &'a mut RemovalQueue<Actor>,
    player_rq: &'a mut RemovalQueue<Player>,
}

impl EntityRemovalCheckSystemData<'_> {
    pub fn run(self) -> bool {
        !(self.actor_rq.is_empty() && self.player_rq.is_empty())
    }
}

pub struct EntityRemovalSystem;

impl System for EntityRemovalSystem {
    type Data<'a> = EntityRemovalSystemData<'a>;
}

#[derive(SystemData)]
pub struct EntityRemovalSystemData<'a> {
    snapshot: &'a ServerSnapshot,

    class_ac: &'a mut ClassActorComponent,
    position_ac: &'a mut PositionActorComponent,
    velocity_ac: &'a mut VelocityActorComponent,
    orientation_ac: &'a mut OrientationActorComponent,
    player_ac: &'a mut PlayerActorComponent,
    chunk_activation_ac: &'a mut ChunkActivationActorComponent,
    effect_ac: &'a mut EffectActorComponent,
    projectile_ac: &'a mut ProjectileActorComponent,
    actor_registry: &'a mut ActorRegistry,
    actor_rq: &'a mut RemovalQueue<Actor>,
    player_rq: &'a mut RemovalQueue<Player>,

    client_pc: &'a mut ClientPlayerComponent,
    chunk_update_pc: &'a mut ChunkUpdatePlayerComponent,
    chunk_view_pc: &'a mut ChunkViewPlayerComponent,
    actions_packer_pc: &'a mut ActionsPackerPlayerComponent,
    actor_pc: &'a mut ActorPlayerComponent,
}

impl EntityRemovalSystemData<'_> {
    pub fn run(self) {
        for player in self.player_rq.drain() {
            self.client_pc.remove(&player);
            self.chunk_update_pc.remove(&player);
            self.chunk_view_pc.remove(&player);
            self.actions_packer_pc.remove(&player);
            if let Some(actor) = self.actor_pc.remove(&player) {
                self.actor_rq.enqueue(actor);
            }
        }

        for actor in self.actor_rq.drain() {
            self.class_ac.remove(&actor, *self.snapshot);
            self.position_ac.remove(&actor, *self.snapshot);
            self.velocity_ac.remove(&actor, *self.snapshot);
            self.orientation_ac.remove(&actor, *self.snapshot);
            self.player_ac.remove(&actor);
            self.chunk_activation_ac.remove(&actor);
            self.effect_ac.remove_actor(&actor);
            self.projectile_ac.remove(&actor);
            self.actor_registry.remove(&actor, *self.snapshot);
        }
    }
}
