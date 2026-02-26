use anyhow::Error;
use serde::Deserialize;
use voxbrix_common::{
    component::dimension_kind::DimensionKindComponent,
    entity::chunk::{
        Chunk,
        ChunkRadius,
    },
    math::Vec3I32,
    FromDescriptor,
};
use voxbrix_world::World;

#[derive(Clone, Copy, Deserialize)]
pub struct PlayerChunkView {
    min: Vec3I32,
    max: Vec3I32,
}

impl PlayerChunkView {
    pub fn to_chunk_radius(self, chunk: &Chunk) -> ChunkRadius {
        ChunkRadius::from_boundaries(chunk.dimension, self.min, self.max)
    }
}

impl Default for PlayerChunkView {
    fn default() -> Self {
        Self {
            min: Vec3I32::splat(-10),
            max: Vec3I32::splat(10),
        }
    }
}

impl FromDescriptor for PlayerChunkView {
    type Descriptor = PlayerChunkView;

    const COMPONENT_NAME: &str = "player_chunk_view";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}

pub type PlayerChunkViewDimensionKindComponent = DimensionKindComponent<PlayerChunkView>;
