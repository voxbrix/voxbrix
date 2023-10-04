use crate::component::actor::ActorComponent;

pub struct ActorChunkActivation {
    pub radius: i32,
}

pub type ChunkActivationActorComponent = ActorComponent<ActorChunkActivation>;
