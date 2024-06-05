use crate::{
    entity::block_class::BlockClass,
    AsFromUsize,
};

pub mod collision;
pub mod opacity;

pub struct BlockClassComponent<T> {
    classes: Vec<Option<T>>,
}

impl<T> BlockClassComponent<T> {
    pub fn new() -> Self {
        Self {
            classes: Vec::new(),
        }
    }

    pub fn get(&self, block_class: &BlockClass) -> Option<&T> {
        self.classes.get(block_class.as_usize())?.as_ref()
    }

    pub fn reload(&mut self, data: Vec<Option<T>>) {
        self.classes = data;
    }
}
