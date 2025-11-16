use crate::{
    entity::block_environment::BlockEnvironment,
    system::component_map::ComponentMap,
    AsFromUsize,
    LabelLibrary,
};
use anyhow::Error;
use serde::Deserialize;

pub struct BlockEnvironmentComponent<T> {
    environments: Vec<Option<T>>,
}

impl<T> BlockEnvironmentComponent<T> {
    pub fn new<'de, 'label, D>(
        component_map: &'de ComponentMap<BlockEnvironment>,
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
                .get_label_map_for::<BlockEnvironment>()
                .expect("BlockEnvironment label map is undefined")
                .len(),
            || None,
        );

        for res in component_map.get_component::<'de, 'label, D>(component_name) {
            let (e, d) = res?;

            vec[e.as_usize()] = Some(convert(d)?);
        }

        Ok(Self { environments: vec })
    }

    pub fn get(&self, block_environment: &BlockEnvironment) -> Option<&T> {
        self.environments
            .get(block_environment.as_usize())?
            .as_ref()
    }
}
