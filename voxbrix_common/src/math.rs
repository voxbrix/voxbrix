use std::cmp::Ordering;

pub type Vec3F32 = glam::Vec3;
pub type Vec3I32 = glam::IVec3;
pub type QuatF32 = glam::Quat;
pub type Mat4F32 = glam::Mat4;

#[cfg_attr(rustfmt, rustfmt_skip)]
pub trait Directions {
    const FORWARD: Self;
    const BACK: Self;
    const RIGHT: Self;
    const LEFT: Self;
    const UP: Self;
    const DOWN: Self;
}

#[cfg_attr(rustfmt, rustfmt_skip)]
impl Directions for Vec3F32 {
    const FORWARD: Self = Vec3F32::new(1.0, 0.0, 0.0);
    const BACK: Self = Vec3F32::new(-1.0, 0.0, 0.0);
    const RIGHT: Self = Vec3F32::new(0.0, 1.0, 0.0);
    const LEFT: Self = Vec3F32::new(0.0, -1.0, 0.0);
    const UP: Self = Vec3F32::new(0.0, 0.0, 1.0);
    const DOWN: Self = Vec3F32::new(0.0, 0.0, -1.0);
}

/// Round in the required direction
pub trait Round {
    /// Rounding to the higher value: e.g. `1.5f32.round_up() == 2i32`, `(-1.5f32).round_up() == -1i32`
    fn round_up(self) -> i32;
    /// Rounding to the lower value: e.g. `1.5f32.round_down() == 1i32`, `(-1.5f32).round_down() == -2i32`
    fn round_down(self) -> i32;
}

impl Round for f32 {
    fn round_up(self) -> i32 {
        match self.partial_cmp(&0.0) {
            Some(Ordering::Less) => self as i32,
            Some(Ordering::Greater) => self as i32 + 1,
            _ => self as i32,
        }
    }

    fn round_down(self) -> i32 {
        match self.partial_cmp(&0.0) {
            Some(Ordering::Less) => self as i32 - 1,
            Some(Ordering::Greater) => self as i32,
            _ => self as i32,
        }
    }
}

pub trait MinMax {
    const MIN: Self;
    const MAX: Self;
}
