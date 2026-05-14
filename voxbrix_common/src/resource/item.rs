use crate::entity::item_class::ItemClass;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Debug)]
pub struct Item {
    pub class: ItemClass,
    pub metadata: u64,
}
