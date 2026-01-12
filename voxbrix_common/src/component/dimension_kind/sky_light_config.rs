use crate::{
    component::dimension_kind::DimensionKindComponent,
    FromDescriptor,
};
use anyhow::Error;
use serde::{
    de::{
        Deserializer,
        Error as _,
    },
    Deserialize,
};
use voxbrix_world::World;

#[derive(PartialEq, Debug)]
pub struct SkyLightConfig {
    pub side: usize,
}

impl<'de> Deserialize<'de> for SkyLightConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SkyLightConfig {
            side: usize,
        }

        let inner = SkyLightConfig::deserialize(deserializer)?;

        if inner.side > 5 {
            return Err(D::Error::custom(
                "side index must be within (0 ..= 5) range",
            ));
        }

        Ok(Self { side: inner.side })
    }
}

impl FromDescriptor for SkyLightConfig {
    type Descriptor = SkyLightConfig;

    const COMPONENT_NAME: &str = "sky_light";

    fn from_descriptor(desc: Option<Self::Descriptor>, _world: &World) -> Result<Self, Error> {
        desc.ok_or_else(|| Error::msg("sky_light descriptor is missing"))
    }
}

pub type SkyLightConfigDimensionKindComponent = DimensionKindComponent<Option<SkyLightConfig>>;
