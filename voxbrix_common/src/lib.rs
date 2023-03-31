pub mod component;
pub mod entity;
pub mod math;
pub mod messages;
pub mod pack;
pub mod sparse_vec;
pub mod stream;
pub mod system;

use component::block::BlocksVec;
use entity::{
    block_class::BlockClass,
    chunk::Chunk,
};
use serde::{
    Deserialize,
    Serialize,
};

#[macro_export]
macro_rules! unblock {
    (($($a:ident),+)$e:expr) => {
        {
            let res;

            (($($a),+), res) = blocking::unblock(move || {
                let res = $e;
                (($($a),+), res)
            }).await;

            res
        }
    };
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChunkData {
    pub chunk: Chunk,
    pub block_classes: BlocksVec<BlockClass>,
}
