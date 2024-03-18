use nohash_hasher::IsEnabled;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Script(pub u64);

impl IsEnabled for Script {}
