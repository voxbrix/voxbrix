use crate::entity::block_class::BlockClass;

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

    // pub fn set(&mut self, class: BlockClass, new: T) {
    // let i = class.index();
    // if self.classes.len() > i {
    // self.classes[i] = Some(new);
    // } else {
    // self.classes.resize_with(i, || None);
    // self.classes.push(Some(new));
    // }
    // }

    pub fn get(&self, i: BlockClass) -> Option<&T> {
        self.classes.get(i.index())?.as_ref()
    }

    pub fn reload(&mut self, data: Vec<Option<T>>) {
        self.classes = data;
    }
}
