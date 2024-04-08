use crate::{
    component::actor::position::LocalPosition,
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
use std::{
    ops::{
        Add,
        Mul,
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Velocity {
    pub vector: Vec3F32,
}

impl Encode for Velocity {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        Encode::encode(&self.vector.to_array(), encoder)?;
        Ok(())
    }
}

impl Decode for Velocity {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self {
            vector: Vec3F32::from_array(Decode::decode(decoder)?),
        })
    }
}

bincode::impl_borrow_decode!(Velocity);

impl Add<Velocity> for Velocity {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Velocity {
            vector: self.vector + other.vector,
        }
    }
}

impl Mul<Duration> for Velocity {
    type Output = LocalPosition;

    fn mul(self, other: Duration) -> LocalPosition {
        LocalPosition {
            vector: self.vector * other.as_secs_f32(),
        }
    }
}
