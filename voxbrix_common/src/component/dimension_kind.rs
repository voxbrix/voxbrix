use crate::{
    entity::chunk::DimensionKind,
    system::component_map::ComponentMap,
    AsFromUsize,
    LabelLibrary,
};
use anyhow::Error;
use serde::Deserialize;

pub mod sky_light_config;

pub struct DimensionKindComponent<T> {
    data: Vec<T>,
}

impl<T> DimensionKindComponent<T> {
    pub fn new<'de, 'label, D>(
        component_map: &'de ComponentMap<DimensionKind>,
        label_library: &LabelLibrary,
        component_name: &'label str,
        convert: impl Fn(D) -> Result<T, Error>,
    ) -> Result<Self, Error>
    where
        T: Default,
        D: Deserialize<'de>,
        'label: 'de,
    {
        let mut data = Vec::new();

        data.resize_with(
            label_library
                .get_label_map_for::<DimensionKind>()
                .expect("DimensionKind label map is undefined")
                .len(),
            || Default::default(),
        );

        for res in component_map.get_component::<'de, 'label, D>(component_name) {
            let (e, d) = res?;

            data[e.as_usize()] = convert(d)?;
        }

        Ok(Self { data })
    }

    pub fn get(&self, dimension_kind: &DimensionKind) -> &T {
        self.data.get(dimension_kind.as_usize()).unwrap()
    }
}
