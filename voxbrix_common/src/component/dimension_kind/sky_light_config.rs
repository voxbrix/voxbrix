use crate::component::dimension_kind::DimensionKindComponent;
use serde::{
    de::{
        Deserializer,
        Error,
    },
    Deserialize,
};

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

pub type SkyLightConfigDimensionKindComponent = DimensionKindComponent<Option<SkyLightConfig>>;
