use crate::entity::block_class::BlockClass;

pub struct BlockClassComponent<T> {
    classes: Vec<Option<T>>,
}

impl<T> BlockClassComponent<T> {
    pub fn new() -> Self {
        Self {
            classes: Vec::new(),
        }
    }

    pub fn set(&mut self, i: BlockClass, new: T) {
        if self.classes.len() > i.0 {
            self.classes[i.0] = Some(new);
        } else {
            self.classes.resize_with(i.0, || None);
            self.classes.push(Some(new));
        }
    }

    pub fn get(&self, i: BlockClass) -> Option<&T> {
        self.classes.get(i.0)?.as_ref()
    }
}
