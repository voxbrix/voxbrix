use crate::{
    entity::block_class::BlockClass,
    system::component_map::ComponentMap,
    AsFromUsize,
    LabelLibrary,
};
use anyhow::Error;
use serde::Deserialize;

pub mod collision;
pub mod opacity;

pub struct BlockClassComponent<T> {
    classes: Vec<Option<T>>,
}

impl<T> BlockClassComponent<T> {
    pub fn new<'de, 'label, D>(
        component_map: &'de ComponentMap<BlockClass>,
        label_library: &LabelLibrary,
        component_name: &'label str,
        convert: impl Fn(D) -> Result<T, Error>,
    ) -> Result<Self, Error>
    where
        D: Deserialize<'de>,
        'label: 'de,
    {
        let mut vec = Vec::new();

        vec.resize_with(
            label_library
                .get_label_map_for::<BlockClass>()
                .expect("BlockClass label map is undefined")
                .len(),
            || None,
        );

        for res in component_map.get_component::<'de, 'label, D>(component_name) {
            let (e, d) = res?;

            vec[e.as_usize()] = Some(convert(d)?);
        }

        Ok(Self { classes: vec })
    }

    pub fn get(&self, block_class: &BlockClass) -> Option<&T> {
        self.classes.get(block_class.as_usize())?.as_ref()
    }
}
