use voxbrix_world::{
    Initialization,
    World,
};

pub struct PlayerActorMovementMetadata {
    pub stands_on_surface: bool,
}

impl Initialization for PlayerActorMovementMetadata {
    type Error = anyhow::Error;

    async fn initialization(_world: &World) -> Result<Self, Self::Error> {
        Ok(Self {
            stands_on_surface: false,
        })
    }
}
