use std::mem;

pub const KEY_LENGTH: usize = mem::size_of::<u64>();

#[derive(PartialOrd, Ord, PartialEq, Eq, Clone, Copy, Debug)]
pub struct Player(pub u64);
