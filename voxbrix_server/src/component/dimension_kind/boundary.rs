use anyhow::Error;
use serde::Deserialize;
use voxbrix_common::{
    component::dimension_kind::DimensionKindComponent,
    entity::chunk::Chunk,
    math::Vec3I32,
    FromDescriptor,
};
use voxbrix_world::World;

#[derive(Clone, Copy, Deserialize)]
pub struct Boundary {
    min: Vec3I32,
    max: Vec3I32,
}

impl Boundary {
    /// Does NOT compare dimension.
    pub fn is_chunk_within(&self, chunk: &Chunk) -> bool {
        chunk.position.x >= self.min.x
            && chunk.position.x <= self.max.x
            && chunk.position.y >= self.min.y
            && chunk.position.y <= self.max.y
            && chunk.position.z >= self.min.z
            && chunk.position.z <= self.max.z
    }
}

impl Default for Boundary {
    fn default() -> Self {
        Self {
            min: Vec3I32::splat(i32::MIN),
            max: Vec3I32::splat(i32::MAX),
        }
    }
}

impl FromDescriptor for Boundary {
    type Descriptor = Boundary;

    const COMPONENT_NAME: &str = "boundary";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}

pub type BoundaryDimensionKindComponent = DimensionKindComponent<Boundary>;
