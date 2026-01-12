use crate::{
    component::block_class::BlockClassComponent,
    FromDescriptor,
};
use anyhow::Error;
use serde::Deserialize;
use voxbrix_world::World;

pub type OpacityBlockClassComponent = BlockClassComponent<Opacity>;

#[derive(Deserialize, Default, Debug)]
#[serde(tag = "kind")]
pub enum Opacity {
    #[default]
    None,
    Full,
}

impl FromDescriptor for Opacity {
    type Descriptor = Opacity;

    const COMPONENT_NAME: &str = "opacity";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
