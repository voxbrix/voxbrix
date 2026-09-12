use crate::{
    component::StaticEntityComponent,
    entity::chunk::DimensionKind,
};

pub mod gravity;
pub mod player_chunk_view;
pub mod sky_light_config;

pub type DimensionKindComponent<T> = StaticEntityComponent<DimensionKind, T>;
