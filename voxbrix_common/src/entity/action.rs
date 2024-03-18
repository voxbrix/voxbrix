use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Hash, Ord, Copy, Clone, Debug)]
pub struct Action(pub u64);

impl nohash_hasher::IsEnabled for Action {}
