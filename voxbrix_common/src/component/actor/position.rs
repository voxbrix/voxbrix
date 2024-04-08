use crate::{
    entity::chunk::Chunk,
    math::Vec3F32,
};
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
use std::ops::Add;

#[derive(PartialEq, Clone, Copy, Debug)]
pub struct Position {
    pub chunk: Chunk,
    pub offset: Vec3F32,
}

impl Encode for Position {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        Encode::encode(&self.chunk, encoder)?;
        Encode::encode(&self.offset.to_array(), encoder)?;
        Ok(())
    }
}

impl Decode for Position {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self {
            chunk: Decode::decode(decoder)?,
            offset: Vec3F32::from_array(Decode::decode(decoder)?),
        })
    }
}

bincode::impl_borrow_decode!(Position);

#[derive(Clone, Debug)]
pub struct LocalPosition {
    pub vector: Vec3F32,
}

impl Add<LocalPosition> for LocalPosition {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        LocalPosition {
            vector: self.vector + other.vector,
        }
    }
}
