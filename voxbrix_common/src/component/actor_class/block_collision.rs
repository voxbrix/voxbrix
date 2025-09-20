use serde::{
    Deserialize,
    Serialize,
};

#[derive(PartialEq, Serialize, Deserialize, Debug)]
#[serde(tag = "kind")]
pub enum BlockCollision {
    AABB { radius_blocks: [f32; 3] },
}
