use bincode::{
    Decode,
    Encode,
};

#[derive(Encode, Decode, PartialEq, Eq, PartialOrd, Hash, Ord, Copy, Clone, Debug)]
pub struct Action(pub u64);

impl nohash_hasher::IsEnabled for Action {}
