#[cfg(feature = "script")]
use crate::blocks_in_chunk;
use serde::{
    de::{
        Deserializer,
        Error as _,
    },
    ser::Serializer,
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct DimensionKind(pub u32);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Dimension {
    pub kind: DimensionKind,
    pub phase: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Chunk {
    pub position: [i32; 3],
    pub dimension: Dimension,
}

#[derive(Clone, Copy, Debug)]
pub struct Block(usize);

impl Block {
    #[cfg(feature = "script")]
    pub fn from_usize(value: usize) -> Option<Self> {
        if value >= blocks_in_chunk() {
            return None;
        }

        Some(Self(value))
    }

    #[cfg(not(feature = "script"))]
    pub fn from_usize(value: usize) -> Self {
        Self(value)
    }

    pub fn as_usize(&self) -> usize {
        self.0
    }
}

#[cfg(feature = "script")]
impl<'de> Deserialize<'de> for Block {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let block: usize = u16::deserialize(deserializer)?
            .try_into()
            .map_err(|_| D::Error::custom("Block value out of bounds"))?;

        if block > blocks_in_chunk() {
            return Err(D::Error::custom("Block value out of bounds of chunk"));
        }

        Ok(Block(block))
    }
}

#[cfg(not(feature = "script"))]
impl<'de> Deserialize<'de> for Block {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let block: usize = u16::deserialize(deserializer)?
            .try_into()
            .map_err(|_| D::Error::custom("Block value out of bounds"))?;

        Ok(Block(block))
    }
}

impl Serialize for Block {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let value: u16 = self.0.try_into().unwrap();
        value.serialize(serializer)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetTargetBlockRequest {
    pub chunk: Chunk,
    pub offset: [f32; 3],
    pub direction: [f32; 3],
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct BlockClass(pub u32);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Actor(pub u32);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Action(pub u32);

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ActionInput<'a> {
    pub action: Action,
    pub actor: Option<Actor>,
    pub data: &'a [u8],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetTargetBlockResponse {
    pub chunk: Chunk,
    pub block: Block,
    pub side: u8,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SetClassOfBlockRequest {
    pub chunk: Chunk,
    pub block: Block,
    pub block_class: BlockClass,
}
