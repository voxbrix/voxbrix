use std::sync::Mutex;

pub struct RemovalQueue<T>(Mutex<Vec<T>>);

impl<T> RemovalQueue<T> {
    pub fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    pub fn enqueue(&self, entity: T) {
        self.0.lock().unwrap().push(entity);
    }

    pub fn drain<'a>(&'a mut self) -> impl ExactSizeIterator<Item = T> + 'a {
        self.0.get_mut().unwrap().drain(..)
    }

    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }
}

impl<T> Default for RemovalQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}
