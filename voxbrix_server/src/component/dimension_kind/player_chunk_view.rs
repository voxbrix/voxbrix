use serde::Deserialize;
use voxbrix_common::{
    component::dimension_kind::DimensionKindComponent,
    entity::chunk::{
        Chunk,
        ChunkRadius,
    },
    math::Vec3I32,
};

#[derive(Clone, Copy, Deserialize)]
pub struct PlayerChunkView {
    min: Vec3I32,
    max: Vec3I32,
}

impl PlayerChunkView {
    pub fn into_chunk_radius(&self, chunk: &Chunk) -> ChunkRadius {
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

pub type PlayerChunkViewDimensionKindComponent = DimensionKindComponent<PlayerChunkView>;
