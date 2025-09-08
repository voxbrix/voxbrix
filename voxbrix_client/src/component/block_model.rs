use crate::entity::block_model::BlockModel;
use anyhow::Error;
use serde::Deserialize;
use voxbrix_common::{
    system::component_map::ComponentMap,
    AsFromUsize,
    LabelLibrary,
};

pub mod builder;
pub mod culling;

pub struct BlockModelComponent<T> {
    data: Vec<Option<T>>,
}

impl<T> BlockModelComponent<T> {
    pub fn new<'de, 'label, D>(
        component_map: &'de ComponentMap<BlockModel>,
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
                .get_label_map_for::<BlockModel>()
                .expect("BlockClass label map is undefined")
                .len(),
            || None,
        );

        for res in component_map.get_component::<'de, 'label, D>(component_name) {
            let (e, d) = res?;

            vec[e.as_usize()] = Some(convert(d)?);
        }

        Ok(Self { data: vec })
    }

    pub fn get(&self, block_model: &BlockModel) -> Option<&T> {
        self.data.get(block_model.as_usize())?.as_ref()
    }
}
