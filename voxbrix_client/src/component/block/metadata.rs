use voxbrix_common::component::block::{
    metadata::BlockMetadata,
    BlockComponentSimple,
    BlocksVec,
};

pub type MetadataBlockComponent = BlockComponentSimple<BlocksVec<BlockMetadata>>;
