use crate::{
    component::block_environment::BlockEnvironmentComponent,
    FromDescriptor,
};
use anyhow::Error;
use serde::{
    Deserialize,
    Serialize,
};
use voxbrix_world::World;

pub type DensityBlockEnvironmentComponent = BlockEnvironmentComponent<Density>;

#[derive(PartialEq, Serialize, Deserialize, Debug, Default, Clone, Copy)]
pub struct Density(pub f32);

impl FromDescriptor for Density {
    type Descriptor = Density;

    const COMPONENT_NAME: &str = "density";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        Ok(desc.unwrap_or_default())
    }
}
