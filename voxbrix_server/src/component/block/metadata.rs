use crate::{
    component::block::TrackingBlockComponent,
    storage::TypeName,
};
use voxbrix_common::component::block::{
    metadata::BlockMetadata,
    BlocksVec,
};

pub type MetadataBlockComponent = TrackingBlockComponent<BlocksVec<BlockMetadata>>;

impl TypeName for BlocksVec<BlockMetadata> {
    const NAME: &'static str = "BlocksVec<BlockMetadata>";
}
