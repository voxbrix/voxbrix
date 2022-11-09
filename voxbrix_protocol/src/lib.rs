use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    io::Cursor,
};

#[cfg(any(feature = "client", test))]
pub mod client;

#[cfg(any(feature = "server", test))]
pub mod server;

pub const MAX_PACKET_SIZE: usize = 508;
pub const MAX_HEADER_SIZE: usize = 48;

// 508 - channel(16) - sender(16) - assign_id(16)
pub const MAX_DATA_SIZE: usize = 460;

const NEW_CONNECTION_ID: usize = 0;
const SERVER_ID: usize = 1;

trait AsSlice<T> {
    fn slice(&self) -> &[T];
}

trait AsMutSlice<T> {
    fn mut_slice(&mut self) -> &mut [T];
}

impl<'a, T> AsSlice<T> for Cursor<&[T]> {
    fn slice(&self) -> &[T] {
        &self.get_ref()[.. self.position() as usize]
    }
}

impl<'a, T> AsSlice<T> for Cursor<&mut [T]> {
    fn slice(&self) -> &[T] {
        &self.get_ref()[.. self.position() as usize]
    }
}

impl<T> AsMutSlice<T> for Cursor<&mut [T]> {
    fn mut_slice(&mut self) -> &mut [T] {
        let len = self.position() as usize;
        &mut self.get_mut()[.. len]
    }
}

pub struct Buffer {
    buffer: [u8; MAX_PACKET_SIZE],
    start: usize,
    stop: usize,
}

impl Buffer {
    pub fn as_ref(&self) -> &[u8] {
        &self.buffer[self.start .. self.stop]
    }

    pub fn as_mut(&mut self) -> &mut [u8] {
        &mut self.buffer[self.start .. self.stop]
    }
}

pub struct Packet {
    data: Data,
}

impl From<Buffer> for Packet {
    fn from(from: Buffer) -> Self {
        Packet {
            data: Data::Single(from),
        }
    }
}

impl From<Vec<u8>> for Packet {
    fn from(from: Vec<u8>) -> Self {
        Packet {
            data: Data::Collection(from),
        }
    }
}

enum Data {
    Collection(Vec<u8>),
    Single(Buffer),
}

impl AsRef<[u8]> for Packet {
    fn as_ref(&self) -> &[u8] {
        match &self.data {
            Data::Collection(v) => v.as_ref(),
            Data::Single(a) => a.as_ref(),
        }
    }
}

impl AsMut<[u8]> for Packet {
    fn as_mut(&mut self) -> &mut [u8] {
        match &mut self.data {
            Data::Collection(v) => v.as_mut(),
            Data::Single(a) => a.as_mut(),
        }
    }
}

struct UnreliableBuffer {
    split_id: u16,
    expected_length: usize,
    existing_pieces: BTreeSet<usize>,
    buffer: BTreeMap<usize, (usize, [u8; MAX_DATA_SIZE])>,
}

#[macro_export]
macro_rules! seek_read {
    ($e:expr, $c:literal) => {
        match $e {
            Ok(r) => r,
            Err(_) => {
                log::warn!("read {} error", $c);
                continue;
            },
        }
    };
}

#[macro_export]
macro_rules! seek_write {
    ($e:expr, $c:literal) => {
        match $e {
            Ok(r) => r,
            Err(_) => {
                log::warn!("write {} error", $c);
                continue;
            },
        }
    };
}

pub type Id = usize;
pub type Sequence = u16;
pub type Channel = usize;

struct Type;

#[rustfmt::skip]
impl Type {
    const CONNECT: u8 = 0;

    const ASSIGN_ID: u8 = 1;
        // id: usize,

    const ACKNOWLEDGE: u8 = 2;
        // sequence: u16

    const DISCONNECT: u8 = 3;

    const PING: u8 = 4;
        // sequence: u16,

    const UNRELIABLE: u8 = 5;
        // channel: usize,
        // data: &[u8],

    const UNRELIABLE_SPLIT_START: u8 = 6;
        // channel: usize,
        // split_id: u16,
        // length: usize,
        // data: &[u8],

    const UNRELIABLE_SPLIT: u8 = 7;
        // channel: usize,
        // split_id: u16,
        // count: usize,
        // data: &[u8],

    const RELIABLE: u8 = 8;
        // channel: usize,
        // sequence: u16,
        // data: &[u8],

    const RELIABLE_SPLIT: u8 = 9;
        // channel: usize,
        // sequence: u16,
        // data: &[u8],

    const UNDEFINED: u8 = u8::MAX;
}

#[cfg(test)]
mod tests {
    use crate::{
        client::Client,
        server::Server,
    };
    use async_executor::LocalExecutor;
    use async_io::Timer;
    use futures_lite::future;
    use std::{
        cell::RefCell,
        iter,
        sync::atomic::{
            AtomicU16,
            Ordering,
        },
        time::Duration,
    };

    static TEST_NUM_DISPENCER: AtomicU16 = AtomicU16::new(0);

    #[test]
    fn unreliable_test_0() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");
                loop {
                    let (mut tx, _rx) = server.accept().await.expect("connection accepted");

                    rt.spawn(async move {
                        tx.send_unreliable(0, b"HelloWorld")
                            .await
                            .expect("server sent packet");
                    })
                    .detach();
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (_tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 508];

            let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");

            assert_eq!(result.as_ref(), b"HelloWorld");
            assert_eq!(channel, 0);
        }));
    }

    #[test]
    fn unreliable_test_1() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let data = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));

        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");
                loop {
                    let (mut tx, _rx) = server.accept().await.expect("connection accepted");

                    let data = &data;

                    rt.spawn(async move {
                        tx.send_unreliable(0, data)
                            .await
                            .expect("server sent packet");
                    })
                    .detach();
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (_tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 508];

            let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");

            assert_eq!(result.as_ref(), data.as_slice());
            assert_eq!(channel, 0);
        }));
    }

    #[test]
    fn unreliable_test_2() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));
        let data = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));

        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");
                loop {
                    let (_tx, mut rx) = server.accept().await.expect("connection accepted");

                    let data = &data;

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        let (channel, result) = rx.recv().await.expect("server received data");

                        assert_eq!(result.as_ref(), data.as_slice());
                        assert_eq!(channel, 1);
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (mut tx, _rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            tx.send_unreliable(1, &data)
                .await
                .expect("client sent data");

            task.borrow_mut().take().unwrap().await;
        }));
    }

    #[test]
    fn unreliable_test_3() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));
        let data = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));

        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");
                loop {
                    let (_tx, mut rx) = server.accept().await.expect("connection accepted");

                    let data = &data;

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        for i in 0 .. 10 {
                            let (channel, result) = rx.recv().await.expect("server received data");

                            assert_eq!(
                                result.as_ref(),
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice()
                            );
                            assert_eq!(channel, 2);
                        }
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (mut tx, _rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            for i in 0 .. 10 {
                tx.send_unreliable(
                    2,
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice(),
                )
                .await
                .expect("client sent data");
            }

            task.borrow_mut().take().unwrap().await;
        }));
    }

    #[test]
    fn unreliable_test_4() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));
        let data = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));

        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");
                loop {
                    let (mut tx, mut rx) = server.accept().await.expect("connection accepted");

                    let data = &data;

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        for i in 20 .. 30 {
                            tx.send_unreliable(
                                5,
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice(),
                            )
                            .await
                            .expect("server sent data");
                        }
                        for i in 50 .. 60 {
                            tx.send_unreliable(
                                5,
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice(),
                            )
                            .await
                            .expect("server sent data");
                        }
                        for i in 0 .. 10 {
                            let (channel, result) = rx.recv().await.expect("server received data");

                            assert_eq!(
                                result.as_ref(),
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice()
                            );
                            assert_eq!(channel, 7);
                        }
                        for i in 90 .. 100 {
                            let (channel, result) = rx.recv().await.expect("server received data");

                            assert_eq!(
                                result.as_ref(),
                                [data.as_slice(), &[i]]
                                    .into_iter()
                                    .flatten()
                                    .map(|i| *i)
                                    .collect::<Vec<u8>>()
                                    .as_slice()
                            );
                            assert_eq!(channel, 7);
                        }
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (mut tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut recv_buf = Vec::new();

            for i in 20 .. 30 {
                recv_buf.clear();
                let (channel, result) = rx.recv(&mut recv_buf).await.expect("client received data");

                assert_eq!(
                    result.as_ref(),
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice()
                );
                assert_eq!(channel, 5);
            }

            for i in 50 .. 60 {
                recv_buf.clear();
                let (channel, result) = rx.recv(&mut recv_buf).await.expect("client received data");

                assert_eq!(
                    result.as_ref(),
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice()
                );
                assert_eq!(channel, 5);
            }

            for i in 0 .. 10 {
                tx.send_unreliable(
                    7,
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice(),
                )
                .await
                .expect("client sent data");
            }

            for i in 90 .. 100 {
                tx.send_unreliable(
                    7,
                    [data.as_slice(), &[i]]
                        .into_iter()
                        .flatten()
                        .map(|i| *i)
                        .collect::<Vec<u8>>()
                        .as_slice(),
                )
                .await
                .expect("client sent data");
            }

            task.borrow_mut().take().unwrap().await;
        }));
    }

    #[test]
    fn reliable_test_0() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));
        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");
                loop {
                    let (mut tx, mut rx) = server.accept().await.expect("connection accepted");

                    rt.spawn(async move { while let Ok(_) = rx.recv().await {} })
                        .detach();

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        tx.send_reliable(0, b"HelloWorld")
                            .await
                            .expect("server sent packet");
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (_tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 508];

            let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");

            assert_eq!(result, b"HelloWorld");
            assert_eq!(channel, 0);

            task.borrow_mut().take().unwrap().await;
        }));
    }

    #[test]
    fn reliable_test_1() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));
        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");
                loop {
                    let (mut tx, _rx) = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        tx.send_reliable(0, b"HelloWorld")
                            .await
                            .expect("server sent packet");
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (_tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 508];

            let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");

            assert_eq!(result, b"HelloWorld");
            assert_eq!(channel, 0);

            task.borrow_mut().take().unwrap().await;
        }));
    }

    #[test]
    fn reliable_test_2() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));
        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");
                loop {
                    let (mut tx, _rx) = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        for i in 0 .. 1000 {
                            tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                                .await
                                .expect("server sent packet");
                        }
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (_tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 508];

            for i in 0 .. 1000 {
                let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");
                assert_eq!(result, format!("HelloWorld{}", i).as_bytes());
                assert_eq!(channel, 0);
            }

            task.borrow_mut().take().unwrap().await;
        }));
    }

    #[test]
    fn reliable_test_3() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));
        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");

                loop {
                    let (mut tx, mut rx) = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        for i in 0 .. 1000 {
                            tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                                .await
                                .expect("server sent packet");
                        }

                        for i in 0 .. 1000 {
                            let (channel, result) =
                                rx.recv().await.expect("client message receive");
                            assert_eq!(result.as_ref(), format!("HelloWorld{}", i).as_bytes());
                            assert_eq!(channel, 0);
                        }
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (mut tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 508];

            for i in 0 .. 1000 {
                let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");
                assert_eq!(result, format!("HelloWorld{}", i).as_bytes());
                assert_eq!(channel, 0);
            }

            rt.spawn(async move {
                while let Ok(_) = rx.recv(&mut buf).await {}
                panic!("recv loop ended");
            })
            .detach();

            for i in 0 .. 1000 {
                tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }

            task.borrow_mut().take().unwrap().await;
        }));
    }

    #[test]
    fn reliable_test_4() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));
        let data = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));
        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");

                loop {
                    let (mut tx, mut rx) = server.accept().await.expect("connection accepted");

                    let data = &data;

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        tx.send_reliable(0, data.as_ref())
                            .await
                            .expect("server sent packet");

                        let (channel, result) = rx.recv().await.expect("client message receive");
                        assert_eq!(result.as_ref(), data.as_slice());
                        assert_eq!(channel, 0);
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (mut tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 1508];

            let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");
            assert_eq!(result.as_ref(), data.as_slice());
            assert_eq!(channel, 0);

            rt.spawn(async move {
                while let Ok(_) = rx.recv(&mut buf).await {}
                panic!("recv loop ended");
            })
            .detach();

            tx.send_reliable(0, data.as_ref())
                .await
                .expect("server sent packet");

            task.borrow_mut().take().unwrap().await;
        }));
    }

    #[test]
    fn reliable_test_5() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        let task = Box::leak(Box::new(RefCell::new(None)));

        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server =
                    Server::bind(([127, 0, 0, 1], server_port)).expect("server socket bind");

                loop {
                    let (mut tx, mut rx) = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        for i in 0 .. 10 {
                            let data = Box::leak(Box::new({
                                let data_slice = &[i + 1, i + 2, i + 3, i + 4, i + 5];
                                iter::repeat(data_slice)
                                    .take(300)
                                    .flatten()
                                    .cloned()
                                    .collect::<Vec<_>>()
                            }));

                            tx.send_reliable(0, data.as_ref())
                                .await
                                .expect("server sent packet");
                        }

                        for i in 0 .. 10 {
                            let (channel, result) =
                                rx.recv().await.expect("client message receive");

                            let data = Box::leak(Box::new({
                                let data_slice = &[i + 1, i + 2, i + 3, i + 4, i + 5];
                                iter::repeat(data_slice)
                                    .take(300)
                                    .flatten()
                                    .cloned()
                                    .collect::<Vec<_>>()
                            }));

                            assert_eq!(result.as_ref(), data.as_slice());
                            assert_eq!(channel, 0);
                        }
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let (mut tx, mut rx) = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 1508];

            for i in 0 .. 10 {
                let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");

                let data = Box::leak(Box::new({
                    let data_slice = &[i + 1, i + 2, i + 3, i + 4, i + 5];
                    iter::repeat(data_slice)
                        .take(300)
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()
                }));

                assert_eq!(result.as_ref(), data.as_slice());
                assert_eq!(channel, 0);
            }

            rt.spawn(async move {
                while let Ok(_) = rx.recv(&mut buf).await {}
                panic!("recv loop ended");
            })
            .detach();

            for i in 0 .. 10 {
                let data = Box::leak(Box::new({
                    let data_slice = &[i + 1, i + 2, i + 3, i + 4, i + 5];
                    iter::repeat(data_slice)
                        .take(300)
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>()
                }));
                tx.send_reliable(0, data.as_ref())
                    .await
                    .expect("server sent packet");
            }

            task.borrow_mut().take().unwrap().await;
        }));
    }
}
