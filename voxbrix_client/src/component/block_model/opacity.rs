use crate::component::block_model::BlockModelComponent;
use serde::Deserialize;

pub type OpacityBlockModelComponent = BlockModelComponent<Opacity>;

// Only meaningful for BlockEnvironment having this model
#[derive(Deserialize)]
pub struct Opacity(pub u8);
