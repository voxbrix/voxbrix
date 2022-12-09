use crate::entity::actor_class::ActorClass;

pub struct ActorClassComponent<T> {
    classes: Vec<Option<T>>,
}

impl<T> ActorClassComponent<T> {
    pub fn new() -> Self {
        Self {
            classes: Vec::new(),
        }
    }

    pub fn set(&mut self, i: ActorClass, new: T) {
        if self.classes.len() > i.0 {
            self.classes[i.0] = Some(new);
        } else {
            self.classes.resize_with(i.0, || None);
            self.classes.push(Some(new));
        }
    }

    pub fn get(&self, i: ActorClass) -> Option<&T> {
        self.classes.get(i.0)?.as_ref()
    }
}
