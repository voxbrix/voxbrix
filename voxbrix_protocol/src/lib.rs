use chacha20poly1305::{
    aead::{
        rand_core::OsRng,
        AeadCore,
        AeadInPlace,
    },
    ChaCha20Poly1305,
};
use integer_encoding::VarIntWriter;
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    io::{
        Cursor,
        Read,
    },
    mem,
};

#[cfg(any(feature = "client", test))]
pub mod client;

#[cfg(any(feature = "server", test))]
pub mod server;

pub const MAX_PACKET_SIZE: usize = 508;

pub const MAX_HEADER_SIZE: usize = mem::size_of::<Id>() // sender
    + 1 // type
    + TAG_SIZE // tag
    + NONCE_SIZE // nonce
    + mem::size_of::<usize>()
    + 2
    + mem::size_of::<usize>();

pub const MAX_DATA_SIZE: usize = MAX_PACKET_SIZE - MAX_HEADER_SIZE;

const SERVER_ID: usize = 0;
const NEW_CONNECTION_ID: usize = 1;

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

macro_rules! seek_read_return {
    ($e:expr, $c:literal) => {
        match $e {
            Ok(r) => r,
            Err(_) => {
                log::warn!("read {} error", $c);
                return Err(());
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
pub type Key = [u8; 33];
const KEY_BUFFER: Key = [0; 33];
pub type Secret = [u8; 32];
const SECRET_BUFFER: Secret = [0; 32];
const TAG_SIZE: usize = 16;
const TAG_BUFFER: [u8; TAG_SIZE] = [0; TAG_SIZE];
const NONCE_SIZE: usize = 12;
const NONCE_BUFFER: [u8; NONCE_SIZE] = [0; NONCE_SIZE];

struct Type;

#[rustfmt::skip]
impl Type {
    const CONNECT: u8 = 0;
        // key: Key,

    const ACCEPT: u8 = 1;
        // key: Key,
        // id: Id,

    const ACKNOWLEDGE: u8 = 2;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // sequence: Sequence,

    const DISCONNECT: u8 = 3;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],

    const UNRELIABLE: u8 = 4;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // data: &[u8],

    const UNRELIABLE_SPLIT_START: u8 = 5;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // split_id: u16,
        // length: usize,
        // data: &[u8],

    const UNRELIABLE_SPLIT: u8 = 6;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // split_id: u16,
        // count: usize,
        // data: &[u8],

    const RELIABLE: u8 = 7;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // sequence: Sequence,
        // data: &[u8],

    const RELIABLE_SPLIT: u8 = 8;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // sequence: Sequence,
        // data: &[u8],

    //const PING: u8 = ?;

    const UNDEFINED: u8 = u8::MAX;
}

// returns tag start and total data length
fn write_in_buffer<F>(
    buffer: &mut [u8; MAX_PACKET_SIZE],
    sender: Id,
    packet_type: u8,
    mut f: F,
) -> (usize, usize)
where
    F: FnMut(&mut Cursor<&mut [u8]>),
{
    let mut cursor = Cursor::new(buffer.as_mut());

    cursor.write_varint(sender).unwrap();
    cursor.write_varint(packet_type).unwrap();
    let tag_start = cursor.position() as usize;
    cursor.set_position((tag_start + TAG_SIZE + NONCE_SIZE) as u64);
    f(&mut cursor);

    (tag_start, cursor.position() as usize)
}

// returns total data length
fn encode_in_buffer(
    buffer: &mut [u8; MAX_PACKET_SIZE],
    cipher: &ChaCha20Poly1305,
    tag_start: usize,
    length: usize,
) {
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let tag_finish = tag_start + TAG_SIZE;
    let encryption_start = tag_finish + NONCE_SIZE;
    (&mut buffer[tag_finish .. encryption_start]).copy_from_slice(&nonce);

    let buffer = &mut buffer[.. length];

    let (buffer_pre_enc, buffer_enc) = buffer.split_at_mut(encryption_start);

    let tag = cipher
        .encrypt_in_place_detached(&nonce, &buffer_pre_enc[.. tag_start], buffer_enc)
        .unwrap();

    buffer[tag_start .. tag_finish].copy_from_slice(&tag);
}

// returns start of a relevant data (that is right after the nonce)
fn decode_in_buffer(
    buffer: &mut [u8],
    tag_start: usize,
    cipher: &ChaCha20Poly1305,
) -> Result<usize, ()> {
    let mut cursor = Cursor::new(&buffer);
    cursor.set_position(tag_start as u64);

    let mut tag = TAG_BUFFER;
    seek_read_return!(cursor.read_exact(&mut tag), "tag");

    let mut nonce = NONCE_BUFFER;
    seek_read_return!(cursor.read_exact(&mut nonce), "nonce");

    let encrypted_start = cursor.position() as usize;

    let (buffer_acc_data, buffer_encrypted) = {
        let (buffer, buffer_encrypted) = buffer.split_at_mut(encrypted_start);
        (&buffer[.. tag_start], buffer_encrypted)
    };

    seek_read_return!(
        cipher.decrypt_in_place_detached(
            (&nonce).into(),
            buffer_acc_data,
            buffer_encrypted,
            (&tag).into(),
        ),
        "decrypted"
    );

    Ok(encrypted_start)
}

#[cfg(test)]
mod tests {
    use crate::{
        client::{
            self,
            Client,
        },
        server::{
            self,
            ServerParameters,
        },
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
        let task = Box::leak(Box::new(RefCell::new(None)));
        future::block_on(rt.run(async {
            rt.spawn(async {
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        sender: mut tx,
                        receiver: mut rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

                    *task.borrow_mut() = Some(rt.spawn(async move {
                        tx.send_unreliable(0, b"1HelloWorld1")
                            .await
                            .expect("server sent packet");

                        let (channel, result) = rx.recv().await.unwrap();

                        assert_eq!(result.as_ref(), b"2HelloWorld2");
                        assert_eq!(channel, 1);
                    }));
                }
            })
            .detach();

            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 508];

            let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");

            assert_eq!(result.as_ref(), b"1HelloWorld1");
            assert_eq!(channel, 0);

            tx.send_unreliable(1, b"2HelloWorld2")
                .await
                .expect("client sent packet");

            task.borrow_mut().take().unwrap().await;
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");
                loop {
                    let server::Connection { sender: mut tx, .. } =
                        server.accept().await.expect("connection accepted");

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

            let client::Connection {
                receiver: mut rx, ..
            } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        receiver: mut rx, ..
                    } = server.accept().await.expect("connection accepted");

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

            let client::Connection { sender: mut tx, .. } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        receiver: mut rx, ..
                    } = server.accept().await.expect("connection accepted");

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

            let client::Connection { sender: mut tx, .. } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        sender: mut tx,
                        receiver: mut rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

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

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");
                loop {
                    let server::Connection {
                        sender: mut tx,
                        receiver: mut rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

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

            let client::Connection {
                receiver: mut rx, ..
            } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");
                loop {
                    let server::Connection { sender: mut tx, .. } =
                        server.accept().await.expect("connection accepted");

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

            let client::Connection {
                receiver: mut rx, ..
            } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");
                loop {
                    let server::Connection { sender: mut tx, .. } =
                        server.accept().await.expect("connection accepted");

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

            let client::Connection {
                receiver: mut rx, ..
            } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");

                loop {
                    let server::Connection {
                        sender: mut tx,
                        receiver: mut rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

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

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");

                loop {
                    let server::Connection {
                        sender: mut tx,
                        receiver: mut rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

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

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client
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
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");

                loop {
                    let server::Connection {
                        sender: mut tx,
                        receiver: mut rx,
                        ..
                    } = server.accept().await.expect("connection accepted");

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

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client
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

    #[test]
    #[cfg(not(debug_assertions))]
    fn reliable_test_6() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        std::thread::spawn(move || {
            let rt = LocalExecutor::new();
            future::block_on(rt.run(async {
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .expect("server socket bind");

                let server::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = server.accept().await.expect("connection accepted");

                let task = async move {
                    for i in 0 .. 10000 {
                        tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                            .await
                            .expect("server sent packet");
                    }

                    for i in 0 .. 10000 {
                        let (channel, result) = rx.recv().await.expect("client message receive");
                        assert_eq!(result.as_ref(), format!("HelloWorld{}", i).as_bytes());
                        assert_eq!(channel, 0);
                    }
                };

                rt.spawn(async move {
                    loop {
                        let _ = server.accept().await.expect("connection accepted");
                    }
                })
                .detach();

                task.await;
            }));
        });

        let rt = Box::leak(Box::new(LocalExecutor::new()));
        future::block_on(rt.run(async {
            Timer::after(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port)).expect("client bound");

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            let mut buf = vec![0u8; 508];

            for i in 0 .. 10000 {
                let (channel, result) = rx.recv(&mut buf).await.expect("client message receive");
                assert_eq!(result, format!("HelloWorld{}", i).as_bytes());
                assert_eq!(channel, 0);
            }

            rt.spawn(async move {
                while let Ok(_) = rx.recv(&mut buf).await {}
                panic!("recv loop ended");
            })
            .detach();

            for i in 0 .. 10000 {
                tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }
        }));
    }
}
