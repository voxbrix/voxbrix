use crate::component::block_class::BlockClassComponent;
use serde::Deserialize;

pub type OpacityBlockClassComponent = BlockClassComponent<Opacity>;

#[derive(Deserialize, Debug)]
#[serde(tag = "kind")]
pub enum Opacity {
    Full,
}
