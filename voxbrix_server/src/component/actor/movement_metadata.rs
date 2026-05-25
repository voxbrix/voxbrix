use crate::component::actor::ActorComponent;

/// Per-actor movement metadata, used by movement-related systems.
///
/// Internal to the server — not synchronized to clients.
#[derive(Default)]
pub struct MovementMetadata {
    #[allow(dead_code)]
    pub stands_on_surface: bool,
}

pub type MovementMetadataActorComponent = ActorComponent<MovementMetadata>;
