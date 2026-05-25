use crate::component::actor::ActorComponent;

/// Per-actor movement metadata. Server-internal, not synchronized to clients.
#[derive(Default)]
pub struct MovementMetadata {
    #[allow(dead_code)]
    pub stands_on_surface: bool,
}

pub type MovementMetadataActorComponent = ActorComponent<MovementMetadata>;
