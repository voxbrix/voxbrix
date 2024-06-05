#[cfg(feature = "script")]
use crate::blocks_in_chunk;
use bincode::{
    de::Decoder,
    enc::Encoder,
    error::{
        DecodeError,
        EncodeError,
    },
    Decode,
    Encode,
};

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct DimensionKind(pub u32);

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct Dimension {
    pub kind: DimensionKind,
    pub phase: u64,
}

#[derive(Encode, Decode, Clone, Copy, Debug)]
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
impl Decode for Block {
    fn decode<D>(decoder: &mut D) -> Result<Self, DecodeError>
    where
        D: Decoder,
    {
        let block: usize = u16::decode(decoder)?
            .try_into()
            .map_err(|_| DecodeError::LimitExceeded)?;

        if block > blocks_in_chunk() {
            return Err(DecodeError::LimitExceeded);
        }

        Ok(Block(block))
    }
}

#[cfg(not(feature = "script"))]
impl Decode for Block {
    fn decode<D>(decoder: &mut D) -> Result<Self, DecodeError>
    where
        D: Decoder,
    {
        let block: usize = u16::decode(decoder)?
            .try_into()
            .map_err(|_| DecodeError::LimitExceeded)?;

        Ok(Block(block))
    }
}

bincode::impl_borrow_decode!(Block);

impl Encode for Block {
    fn encode<E>(&self, encoder: &mut E) -> Result<(), EncodeError>
    where
        E: Encoder,
    {
        let value: u16 = self.0.try_into().unwrap();
        value.encode(encoder)
    }
}

#[derive(Encode, Decode, Debug)]
pub struct GetTargetBlockRequest {
    pub chunk: Chunk,
    pub offset: [f32; 3],
    pub direction: [f32; 3],
}

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct BlockClass(pub u64);

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct Actor(pub u64);

#[derive(Encode, Decode, Debug)]
pub struct GetTargetBlockResponse {
    pub chunk: Chunk,
    pub block: Block,
    pub side: u8,
}

#[derive(Encode, Decode, Debug)]
pub struct SetClassOfBlockRequest {
    pub chunk: Chunk,
    pub block: Block,
    pub block_class: BlockClass,
}
