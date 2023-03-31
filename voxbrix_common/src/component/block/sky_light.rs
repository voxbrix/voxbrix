use crate::component::block::{
    BlockComponent,
    BlocksVec,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyLight(u8);

impl SkyLight {
    const DEFAULT_FADE: u8 = 1;
    pub const MAX: Self = Self(16);
    pub const MIN: Self = Self(0);

    pub fn fade(self) -> Self {
        // TODO If MIN > 0, we need to check it here
        Self(self.0.saturating_sub(Self::DEFAULT_FADE))
    }

    pub fn value(self) -> u8 {
        self.0
    }
}

pub type SkyLightBlockComponent = BlockComponent<BlocksVec<SkyLight>>;
