pub struct RemovalQueue<T>(Vec<T>);

impl<T> RemovalQueue<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn enqueue(&mut self, entity: T) {
        self.0.push(entity);
    }

    pub fn drain<'a>(&'a mut self) -> impl ExactSizeIterator<Item = T> + 'a {
        self.0.drain(..)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
