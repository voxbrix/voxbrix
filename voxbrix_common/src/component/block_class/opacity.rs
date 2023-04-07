use crate::component::block_class::BlockClassComponent;
use serde::Deserialize;

pub type OpacityBlockClassComponent = BlockClassComponent<Opacity>;

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Opacity {
    Full,
}
