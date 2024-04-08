use crate::math::{
    Directions,
    QuatF32,
    Vec3F32,
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

#[derive(PartialEq, Clone, Copy, Debug)]
pub struct Orientation {
    pub rotation: QuatF32,
}

impl Encode for Orientation {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        Encode::encode(&self.rotation.to_array(), encoder)?;
        Ok(())
    }
}

impl Decode for Orientation {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        Ok(Self {
            rotation: QuatF32::from_array(Decode::decode(decoder)?),
        })
    }
}

bincode::impl_borrow_decode!(Orientation);

impl Orientation {
    pub fn forward(&self) -> Vec3F32 {
        self.rotation * Vec3F32::FORWARD
    }

    pub fn right(&self) -> Vec3F32 {
        self.rotation * Vec3F32::RIGHT
    }

    pub fn up(&self) -> Vec3F32 {
        self.rotation * Vec3F32::UP
    }

    pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> Self {
        Self {
            rotation: QuatF32::from_axis_angle(Vec3F32::UP, yaw)
                * QuatF32::from_axis_angle(Vec3F32::LEFT, pitch),
        }
    }
}
