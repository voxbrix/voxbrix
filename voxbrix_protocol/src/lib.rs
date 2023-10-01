//! A relatively simple protocol implementation.
//! The protocol is a thin layer above UDP.
//! It is connection-oriented with the client-server peer relationship.
//!
//! The design goals are:
//!
//! 1. Reliable and unreliable packet transmission.
//! 2. AEAD using ChaCha20-Poly1305 algorithm with ECDH handshake.
//! 3. Simplicity.
//!
//! The crate can be tuned by the following feature flags:
//!
//! 1. `single` optimizes the crate toward usage in a single-threaded runtime. Mutually exclusive
//!    with `multi` feature.
//! 2. `multi` allows the crate to be used with multi-threaded runtimes. Mutually exclusive
//!    with `single` feature.
//! 3. `client` enables [`client`] functionality.
//! 4. `server` enables [`server`] functionality.

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
    time::Duration,
};

#[cfg(any(feature = "client", test))]
pub mod client;

#[cfg(any(feature = "server", test))]
pub mod server;

const MAX_PACKET_SIZE: usize = 508;

const MAX_HEADER_SIZE: usize = mem::size_of::<Id>() // sender
    + 1 // type
    + TAG_SIZE // tag
    + NONCE_SIZE // nonce
    + mem::size_of::<usize>()
    + 2
    + mem::size_of::<usize>();

/// Maximum amount of data bytes that fits into one packet.
/// Unreliable messages sent are recommended to be smaller that this.
pub const MAX_DATA_SIZE: usize = MAX_PACKET_SIZE - MAX_HEADER_SIZE;

const SERVER_ID: usize = 0;
const NEW_CONNECTION_ID: usize = 1;
const UNRELIABLE_BUFFERS: usize = 8;
const RELIABLE_QUEUE_LENGTH: u16 = 256;
const RELIABLE_RESEND_AFTER: Duration = Duration::from_millis(1000);

trait AsSlice<T> {
    fn slice(&self) -> &[T];
}

trait AsMutSlice<T> {
    fn mut_slice(&mut self) -> &mut [T];
}

impl<T> AsSlice<T> for Cursor<&[T]> {
    fn slice(&self) -> &[T] {
        &self.get_ref()[.. self.position() as usize]
    }
}

impl<T> AsSlice<T> for Cursor<&mut [T]> {
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

struct UnreliableBuffer {
    split_id: u16,
    channel: Channel,
    expected_length: usize,
    existing_pieces: BTreeSet<usize>,
    buffer: BTreeMap<usize, (usize, [u8; MAX_DATA_SIZE])>,
    complete: bool,
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

type Id = usize;
type Sequence = u16;
/// Channel id type
pub type Channel = usize;
type Key = [u8; 33];
const KEY_BUFFER: Key = [0; 33];
type Secret = [u8; 32];
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

    const UNDEFINED: u8 = u8::MAX;
}

/// Returns tag start byte and total data length.
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

fn encode_in_buffer(
    buffer: &mut [u8; MAX_PACKET_SIZE],
    cipher: &ChaCha20Poly1305,
    tag_start: usize,
    length: usize,
) {
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let tag_stop = tag_start + TAG_SIZE;
    let encryption_start = tag_stop + NONCE_SIZE;
    buffer[tag_stop .. encryption_start].copy_from_slice(&nonce);

    let buffer = &mut buffer[.. length];

    let (buffer_pre_enc, buffer_enc) = buffer.split_at_mut(encryption_start);

    let tag = cipher
        .encrypt_in_place_detached(&nonce, &buffer_pre_enc[.. tag_start], buffer_enc)
        .unwrap();

    buffer[tag_start .. tag_stop].copy_from_slice(&tag);
}

/// Returns total data length.
fn tag_sign_in_buffer(
    buffer: &mut [u8; MAX_PACKET_SIZE],
    cipher: &ChaCha20Poly1305,
    tag_start: usize,
) -> usize {
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    let tag_stop = tag_start + TAG_SIZE;
    let length = tag_stop + NONCE_SIZE;
    buffer[tag_stop .. length].copy_from_slice(&nonce);

    let tag = cipher
        .encrypt_in_place_detached(&nonce, &buffer[.. tag_start], &mut [])
        .unwrap();

    buffer[tag_start .. tag_stop].copy_from_slice(&tag);

    length
}

/// Returns starting byte of the relevant data (that is right after the nonce).
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
    use std::{
        cell::RefCell,
        iter,
        net::{
            SocketAddr,
            UdpSocket,
        },
        sync::atomic::{
            AtomicU16,
            Ordering,
        },
        thread,
        time::Duration,
    };
    use tokio::{
        task::{
            self,
            LocalSet,
        },
        time,
    };

    static TEST_NUM_DISPENCER: AtomicU16 = AtomicU16::new(0);

    fn create_proxy<F>(
        test_num: u16,
        peer_a: SocketAddr,
        peer_b: SocketAddr,
        filter: F,
    ) -> SocketAddr
    where
        F: Fn(usize, SocketAddr) -> bool + Send + 'static,
    {
        let port = 30000 + test_num * 10 + 2;

        let socket_addr: SocketAddr = ([127, 0, 0, 1], port).into();

        let socket = UdpSocket::bind(&socket_addr).unwrap();

        thread::spawn(move || {
            let mut buf = [0u8; super::MAX_PACKET_SIZE];
            let mut packet_num_a = 0;
            let mut packet_num_b = 0;
            while let Ok((len, addr)) = socket.recv_from(&mut buf) {
                let (send_addr, packet_num) = match addr {
                    addr if addr == peer_a => (peer_b, &mut packet_num_a),
                    addr if addr == peer_b => (peer_a, &mut packet_num_b),
                    _ => continue,
                };

                if filter(*packet_num, addr) {
                    socket.send_to(&buf[.. len], send_addr).unwrap();
                }

                *packet_num += 1;
            }
        });

        socket_addr
    }

    #[tokio::test]
    async fn unreliable_test_0() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));

        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");
                    loop {
                        let server::Connection {
                            sender: mut tx,
                            receiver: mut rx,
                            ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            tx.send_unreliable(0, b"1HelloWorld1")
                                .await
                                .expect("server sent packet");

                            let (channel, result) = rx.recv().await.unwrap();

                            assert_eq!(result.as_ref(), b"2HelloWorld2");
                            assert_eq!(channel, 1);
                        }));
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                let (channel, result) = rx.recv().await.expect("client message receive");

                assert_eq!(result.as_ref(), b"1HelloWorld1");
                assert_eq!(channel, 0);

                tx.send_unreliable(1, b"2HelloWorld2")
                    .await
                    .expect("client sent packet");

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn unreliable_test_1() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let data: &_ = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));

        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");
                    loop {
                        let server::Connection { sender: mut tx, .. } =
                            server.accept().await.expect("connection accepted");

                        task::spawn_local(async move {
                            tx.send_unreliable(0, data)
                                .await
                                .expect("server sent packet");
                        });
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    receiver: mut rx, ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                let (channel, result) = rx.recv().await.expect("client message receive");

                assert_eq!(result.as_ref(), data.as_slice());
                assert_eq!(channel, 0);
            })
            .await;
    }

    #[tokio::test]
    async fn unreliable_test_2() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        let data: &_ = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));

        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");
                    loop {
                        let server::Connection {
                            receiver: mut rx, ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            let (channel, result) = rx.recv().await.expect("server received data");

                            assert_eq!(result.as_ref(), data.as_slice());
                            assert_eq!(channel, 1);
                        }));
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection { sender: mut tx, .. } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                tx.send_unreliable(1, &data)
                    .await
                    .expect("client sent data");

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn unreliable_test_3() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        let data: &_ = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));

        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");
                    loop {
                        let server::Connection {
                            receiver: mut rx, ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            for i in 0 .. 10 {
                                let (channel, result) =
                                    rx.recv().await.expect("server received data");

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
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

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

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn unreliable_test_4() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        let data: &_ = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));

        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");
                    loop {
                        let server::Connection {
                            sender: mut tx,
                            receiver: mut rx,
                            ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
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
                                let (channel, result) =
                                    rx.recv().await.expect("server received data");

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
                                let (channel, result) =
                                    rx.recv().await.expect("server received data");

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
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                for i in 20 .. 30 {
                    let (channel, result) = rx.recv().await.expect("client received data");

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
                    let (channel, result) = rx.recv().await.expect("client received data");

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

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn reliable_test_0() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");
                    loop {
                        let server::Connection {
                            sender: mut tx,
                            receiver: mut rx,
                            ..
                        } = server.accept().await.expect("connection accepted");

                        task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            tx.send_reliable(0, b"HelloWorld")
                                .await
                                .expect("server sent packet");
                        }));
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    receiver: mut rx, ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                let (channel, result) = rx.recv().await.expect("client message receive");

                assert_eq!(result, b"HelloWorld");
                assert_eq!(channel, 0);

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn reliable_test_1() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");
                    loop {
                        let server::Connection { sender: mut tx, .. } =
                            server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            tx.send_reliable(0, b"HelloWorld")
                                .await
                                .expect("server sent packet");
                        }));
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    receiver: mut rx, ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                let (channel, result) = rx.recv().await.expect("client message receive");

                assert_eq!(result, b"HelloWorld");
                assert_eq!(channel, 0);

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn reliable_test_2() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");
                    loop {
                        let server::Connection { sender: mut tx, .. } =
                            server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            for i in 0 .. 1000 {
                                tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                                    .await
                                    .expect("server sent packet");
                            }
                        }));
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    receiver: mut rx, ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                for i in 0 .. 1000 {
                    let (channel, result) = rx.recv().await.expect("client message receive");
                    assert_eq!(result, format!("HelloWorld{}", i).as_bytes());
                    assert_eq!(channel, 0);
                }

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn reliable_test_3() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");

                    loop {
                        let server::Connection {
                            sender: mut tx,
                            receiver: mut rx,
                            ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
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
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                for i in 0 .. 1000 {
                    let (channel, result) = rx.recv().await.expect("client message receive");
                    assert_eq!(result, format!("HelloWorld{}", i).as_bytes());
                    assert_eq!(channel, 0);
                }

                task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

                for i in 0 .. 1000 {
                    tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                        .await
                        .expect("server sent packet");
                }

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn reliable_test_4() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        let data: &_ = Box::leak(Box::new({
            let data_slice = &[1, 2, 3, 4, 5];
            iter::repeat(data_slice)
                .take(300)
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
        }));
        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");

                    loop {
                        let server::Connection {
                            sender: mut tx,
                            receiver: mut rx,
                            ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            tx.send_reliable(0, data.as_ref())
                                .await
                                .expect("server sent packet");

                            let (channel, result) =
                                rx.recv().await.expect("client message receive");
                            assert_eq!(result.as_ref(), data.as_slice());
                            assert_eq!(channel, 0);
                        }));
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                let (channel, result) = rx.recv().await.expect("client message receive");
                assert_eq!(result.as_ref(), data.as_slice());
                assert_eq!(channel, 0);

                task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

                tx.send_reliable(0, data.as_ref())
                    .await
                    .expect("server sent packet");

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn reliable_test_5() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));

        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(([127, 0, 0, 1], server_port))
                        .await
                        .expect("server socket bind");

                    loop {
                        let server::Connection {
                            sender: mut tx,
                            receiver: mut rx,
                            ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            for i in 0 .. 10 {
                                let data: &_ = Box::leak(Box::new({
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

                                let data: &_ = Box::leak(Box::new({
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
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(([127, 0, 0, 1], client_port))
                    .await
                    .expect("client bound");

                let client::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = client
                    .connect(([127, 0, 0, 1], server_port))
                    .await
                    .expect("client connection");

                for i in 0 .. 10 {
                    let (channel, result) = rx.recv().await.expect("client message receive");

                    let data: &_ = Box::leak(Box::new({
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

                task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

                for i in 0 .. 10 {
                    let data: &_ = Box::leak(Box::new({
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

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn reliable_test_reliability() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let amount = 1000;

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let client_addr = ([127, 0, 0, 1], client_port);
        let server_addr = ([127, 0, 0, 1], server_port);

        let proxy_addr = create_proxy(
            test_num,
            client_addr.into(),
            server_addr.into(),
            |i, _addr| i % 3 != 2,
        );

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(server_addr)
                        .await
                        .expect("server socket bind");

                    loop {
                        let server::Connection {
                            sender: tx,
                            receiver: rx,
                            ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            let mut tx = tx;
                            let mut rx = rx;
                            for i in 0 .. amount {
                                tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                                    .await
                                    .expect("server sent packet");
                            }

                            task::spawn_local(async {
                                let mut tx = tx;
                                loop {
                                    time::sleep(Duration::from_millis(500)).await;
                                    tx.send_reliable(0, "serv_ping".as_bytes())
                                        .await
                                        .expect("server sent packet");
                                }
                            });

                            for i in 0 .. amount {
                                let (channel, result) =
                                    rx.recv().await.expect("client message receive");
                                assert_eq!(result.as_ref(), format!("HelloWorld{}", i).as_bytes());
                                assert_eq!(channel, 0);
                            }
                        }));
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(client_addr).await.expect("client bound");

                let client::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = client.connect(proxy_addr).await.expect("client connection");

                for i in 0 .. amount {
                    let (channel, result) = rx.recv().await.expect("client message receive");
                    assert_eq!(result, format!("HelloWorld{}", i).as_bytes());
                    assert_eq!(channel, 0);
                }

                task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

                for i in 0 .. amount {
                    tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                        .await
                        .expect("server sent packet");
                }

                task::spawn_local(async {
                    let mut tx = tx;
                    loop {
                        time::sleep(Duration::from_millis(500)).await;
                        tx.send_reliable(0, "cl_ping".as_bytes())
                            .await
                            .expect("server sent packet");
                    }
                });

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[tokio::test]
    async fn reliable_test_wait_complete() {
        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let amount = 1000;

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        let client_addr = ([127, 0, 0, 1], client_port);
        let server_addr = ([127, 0, 0, 1], server_port);

        let proxy_addr = create_proxy(
            test_num,
            client_addr.into(),
            server_addr.into(),
            |i, _addr| i % 3 != 2,
        );

        let task: &_ = Box::leak(Box::new(RefCell::new(None)));
        LocalSet::new()
            .run_until(async move {
                task::spawn_local(async move {
                    let mut server = ServerParameters::default()
                        .bind(server_addr)
                        .await
                        .expect("server socket bind");

                    loop {
                        let server::Connection {
                            sender: tx,
                            receiver: rx,
                            ..
                        } = server.accept().await.expect("connection accepted");

                        *task.borrow_mut() = Some(task::spawn_local(async move {
                            let mut tx = tx;
                            let mut rx = rx;
                            for i in 0 .. amount {
                                tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                                    .await
                                    .expect("server sent packet");
                            }

                            tx.wait_complete().await.expect("waiting for delivery");

                            for i in 0 .. amount {
                                let (channel, result) =
                                    rx.recv().await.expect("client message receive");
                                assert_eq!(result.as_ref(), format!("HelloWorld{}", i).as_bytes());
                                assert_eq!(channel, 0);
                            }

                            task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });
                        }));
                    }
                });

                time::sleep(Duration::from_millis(5)).await;

                let client = Client::bind(client_addr).await.expect("client bound");

                let client::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = client.connect(proxy_addr).await.expect("client connection");

                for i in 0 .. amount {
                    let (channel, result) = rx.recv().await.expect("client message receive");
                    assert_eq!(result, format!("HelloWorld{}", i).as_bytes());
                    assert_eq!(channel, 0);
                }

                task::spawn_local(async move { while let Ok(_) = rx.recv().await {} });

                for i in 0 .. amount {
                    tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                        .await
                        .expect("server sent packet");
                }

                tx.wait_complete().await.expect("waiting for delivery");

                task.borrow_mut().take().unwrap().await.unwrap();
            })
            .await;
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn reliable_test_load() {
        use tokio::runtime::Builder as RTBuilder;

        let test_num = TEST_NUM_DISPENCER.fetch_add(1, Ordering::Relaxed);

        let client_port = 30000 + test_num * 10 + 1;
        let server_port = 30000 + test_num * 10;

        std::thread::spawn(move || {
            let rt = RTBuilder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(LocalSet::new().run_until(async {
                let mut server = ServerParameters::default()
                    .bind(([127, 0, 0, 1], server_port))
                    .await
                    .expect("server socket bind");

                let server::Connection {
                    sender: mut tx,
                    receiver: mut rx,
                    ..
                } = server.accept().await.expect("connection accepted");

                let task = async move {
                    for i in 0 .. 200000 {
                        tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                            .await
                            .expect("server sent packet");
                    }

                    for i in 0 .. 200000 {
                        let (channel, result) = rx.recv().await.expect("client message receive");
                        assert_eq!(result.as_ref(), format!("HelloWorld{}", i).as_bytes());
                        assert_eq!(channel, 0);
                    }
                };

                task::spawn_local(async move {
                    loop {
                        let _ = server.accept().await.expect("connection accepted");
                    }
                });

                task.await;
            }));
        });

        let rt = RTBuilder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(LocalSet::new().run_until(async {
            time::sleep(Duration::from_millis(5)).await;

            let client = Client::bind(([127, 0, 0, 1], client_port))
                .await
                .expect("client bound");

            let client::Connection {
                sender: mut tx,
                receiver: mut rx,
                ..
            } = client
                .connect(([127, 0, 0, 1], server_port))
                .await
                .expect("client connection");

            for i in 0 .. 200000 {
                let (channel, result) = rx.recv().await.expect("client message receive");
                assert_eq!(result, format!("HelloWorld{}", i).as_bytes());
                assert_eq!(channel, 0);
            }

            task::spawn_local(async move {
                while let Ok(_) = rx.recv().await {}
                panic!("recv loop ended");
            });

            for i in 0 .. 200000 {
                tx.send_reliable(0, format!("HelloWorld{}", i).as_bytes())
                    .await
                    .expect("server sent packet");
            }
        }));
    }
}
