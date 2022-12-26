use flume::Sender;
use std::thread;

pub trait AsKey<const KEY_LENGTH: usize> {
    const KEY_LENGTH: usize = KEY_LENGTH;

    fn write_key(&self, buf: &mut [u8]);
    fn read_key<B>(buf: B) -> Self
    where
        Self: Sized,
        B: AsRef<[u8]>;
    fn to_key(self) -> [u8; KEY_LENGTH];
    fn from_key(from: [u8; KEY_LENGTH]) -> Self;
}

pub struct StorageThread {
    tx: Sender<Box<dyn FnMut(&mut Vec<u8>) + Send>>,
}

impl StorageThread {
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

pub mod player {
    use serde::{
        Deserialize,
        Serialize,
    };
    use voxbrix_common::pack::PackDefault;

    #[derive(Serialize, Deserialize)]
    pub struct Player {
        pub username: String,
        pub password: Vec<u8>,
    }

    impl PackDefault for Player {}
}
