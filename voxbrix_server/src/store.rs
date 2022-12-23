use flume::Sender;
use std::thread;

pub trait AsKey {
    fn to_key(&self, buf: &mut [u8]);
    fn from_key<B>(buf: B) -> Self
    where
        Self: Sized,
        B: AsRef<[u8]>;
}

pub struct StoreThread {
    tx: Sender<Box<dyn FnMut(&mut Vec<u8>) + Send>>,
}

impl StoreThread {
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded::<Box<dyn FnMut(&mut Vec<u8>) + Send>>();
        thread::spawn(move || {
            // Shared buffer to serialize data to db format
            let mut buf = Vec::new();
            while let Ok(mut task) = rx.recv() {
                task(&mut buf);
            }
        });

        Self { tx }
    }

    pub fn execute<F>(&self, task: F)
    where
        F: 'static + FnMut(&mut Vec<u8>) + Send,
    {
        let _ = self.tx.send(Box::new(task));
    }
}
