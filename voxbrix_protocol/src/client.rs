//! Client side of the protocol implementation.
//!
//! # Examples
//!
//! ```no_run
//! use futures_lite::future;
//! use std::time::Duration;
//! use tokio::time;
//! use voxbrix_protocol::client::{
//!     Client,
//!     Connection,
//! };
//!
//! async fn example() {
//!     let client = Client::bind(([127, 0, 0, 1], 12345))
//!         .await
//!         .expect("socket bound");
//!
//!     let Connection {
//!         mut receiver,
//!         mut sender,
//!         ..
//!     } = client
//!         .connect(([127, 0, 0, 1], 12346))
//!         .await
//!         .expect("connected to server");
//!
//!     let recv_future = async {
//!         // For reliable messages to work, the future from recv() method must always be
//!         // polled in loop, even if you do not actually use the incoming messages.
//!         while let Ok(_msg) = receiver.recv().await {
//!             // Do something with the data
//!         }
//!     };
//!
//!     let send_future = async {
//!         sender.send_reliable(0, b"Hello Server!").await;
//!         loop {
//!             // Senders send no data passively by themselves and resending lost messages
//!             // in reliable data transfer happens lazily, right before sending a new one.
//!             // Therefore, it is highly recommended to send some kind of "ping" or "keepalive"
//!             // messages periodically, so the lost packets could be retransmitted even if you
//!             // do not send any meaningful data.
//!             time::sleep(Duration::from_secs(1)).await;
//!             sender.send_reliable(0, b"keepalive").await;
//!         }
//!     };
//!
//!     future::or(recv_future, send_future).await;
//! }
//! ```

use crate::{
    seek_read,
    seek_write,
    AsSlice,
    Channel,
    Id,
    Key,
    Sequence,
    Type,
    UnreliableBuffer,
    UnreliableBufferShard,
    KEY_BUFFER,
    MAX_DATA_SIZE,
    MAX_PACKET_SIZE,
    MAX_SPLIT_DATA_SIZE,
    MAX_SPLIT_PACKETS,
    NEW_CONNECTION_ID,
    RELIABLE_QUEUE_LENGTH,
    RELIABLE_RESEND_AFTER,
    SECRET_BUFFER,
    SERVER_ID,
    UNRELIABLE_BUFFERS,
};
use chacha20poly1305::{
    aead::KeyInit,
    ChaCha20Poly1305,
};
#[cfg(feature = "multi")]
use flume::{
    unbounded as new_channel,
    Receiver as ChannelRx,
    Sender as ChannelTx,
    TryRecvError as TryReceiveError,
};
use integer_encoding::{
    VarIntReader,
    VarIntWriter,
};
use k256::{
    ecdh::EphemeralSecret,
    EncodedPoint,
    PublicKey,
};
#[cfg(feature = "single")]
use local_channel::{
    mpsc::{
        channel as new_channel,
        Receiver as ChannelRx,
        Sender as ChannelTx,
    },
    TryReceiveError,
};
use log::debug;
use rand_core::OsRng;
#[cfg(feature = "single")]
use std::rc::Rc;
#[cfg(feature = "multi")]
use std::sync::Arc as Rc;
use std::{
    alloc::{
        self,
        Layout,
    },
    collections::VecDeque,
    fmt,
    io::{
        Cursor,
        Error as StdIoError,
        Read,
        Write,
    },
    mem,
    net::SocketAddr,
    slice,
    time::Instant,
};
use tokio::{
    net::UdpSocket,
    time,
};

type Buffer = [u8; MAX_PACKET_SIZE];

const ZEROED_BUFFER: Buffer = [0u8; MAX_PACKET_SIZE];

type BoxBuffer = Box<Buffer>;

fn allocate_buffer() -> BoxBuffer {
    // SAFETY: fast and safe way to get Box of [0u8; MAX_PACKET_SIZE]
    // without copying stack to heap (as would be with Box::new())
    // https://doc.rust-lang.org/std/boxed/index.html#memory-layout
    unsafe {
        let layout = Layout::new::<Buffer>();
        let ptr = alloc::alloc(layout);
        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }
        Box::from_raw(ptr.cast())
    }
}

async fn send_ack(buffer: &mut Buffer, shared: &Shared, sequence: Sequence) -> Result<(), Error> {
    let (tag_start, len) = crate::write_in_buffer(buffer, shared.id, Type::ACKNOWLEDGE, |cursor| {
        cursor.write_varint(sequence).unwrap();
    });

    crate::encode_in_buffer(buffer, &shared.cipher, tag_start, len);

    shared.transport.send(&buffer[.. len]).await?;
    Ok(())
}

/// The error that can be returned by senders or by the receiver.
#[derive(Debug)]
pub enum Error {
    /// IO error wrapper.
    Io(StdIoError),
    /// Returned on attempt to send reliable message with the receiver dropped.
    ReceiverWasDropped,
    /// Returned by the receiver in case the server dropped the connection handles.
    Disconnect,
    /// Currently internal variant, should not be returned.
    Timeout,
    /// Peer sent message that is too large.
    PeerMessageTooLarge,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Error {}

impl From<StdIoError> for Error {
    fn from(from: StdIoError) -> Self {
        Self::Io(from)
    }
}

/// Connection builder.
pub struct Client {
    transport: UdpSocket,
}

/// Returned by the `Client::connect()` method on successful connection to the server.
pub struct Connection {
    /// One-time own public key.
    /// In combination with some secret, can be used to verify client's the identity for the server.
    pub self_key: Key,
    /// One-time server public key.
    /// In combination with some secret, can be used to verify the identity of the server.
    pub peer_key: Key,
    /// Sender part that has both reliable and unreliable functionality included.
    pub sender: Sender,
    /// Receiver part.
    pub receiver: Receiver,
}

impl Client {
    /// Bind the local socket.
    pub async fn bind<A>(bind_address: A) -> Result<Self, Error>
    where
        A: Into<SocketAddr>,
    {
        let transport = UdpSocket::bind(bind_address.into()).await?;
        Ok(Self { transport })
    }

    /// Use bound socket to connect to the server.
    pub async fn connect<A>(self, server_address: A) -> Result<Connection, Error>
    where
        A: Into<SocketAddr>,
    {
        let Client { transport } = self;

        transport.connect(server_address.into()).await?;

        let (ack_sender, ack_receiver) = new_channel();

        let keypair = EphemeralSecret::random(&mut OsRng);
        let self_key: Key = EncodedPoint::from(keypair.public_key())
            .as_ref()
            .try_into()
            .unwrap();

        let mut buf = ZEROED_BUFFER;

        let mut write_cursor = Cursor::new(buf.as_mut());

        write_cursor.write_varint(NEW_CONNECTION_ID).unwrap();
        write_cursor.write_varint(Type::CONNECT).unwrap();
        write_cursor.write_all(&self_key).unwrap();

        transport.send(write_cursor.slice()).await?;

        let (peer_key, deciphered_peer_key, id) = loop {
            let len = transport.recv(&mut buf).await?;

            let mut read_cursor = Cursor::new(&buf[.. len]);

            let sender: usize = seek_read!(read_cursor.read_varint(), "sender");

            if sender != SERVER_ID {
                debug!("received non-server message");
                continue;
            }

            let mut packet_type = Type::UNDEFINED;
            seek_read!(
                read_cursor.read_exact(slice::from_mut(&mut packet_type)),
                "type"
            );

            if packet_type == Type::ACCEPT {
                let mut key = KEY_BUFFER;

                seek_read!(read_cursor.read_exact(&mut key), "peer key");
                let id: usize = seek_read!(read_cursor.read_varint(), "id");

                let deciphered_peer_key =
                    seek_read!(PublicKey::from_sec1_bytes(&key), "deciphered peer key");

                break (key, deciphered_peer_key, id);
            }
        };

        let mut secret = SECRET_BUFFER;
        keypair
            .diffie_hellman(&deciphered_peer_key)
            .extract::<sha2::Sha256>(None)
            .expand(&[], &mut secret)
            .unwrap();

        let cipher = ChaCha20Poly1305::new((&secret).into());

        let shared = {
            let shared = Shared {
                id,
                cipher,
                transport,
            };

            Rc::new(shared)
        };

        let receiver = Receiver {
            shared: shared.clone(),
            sequence: 0,
            reliable_queue: vec![None; RELIABLE_QUEUE_LENGTH as usize].into(),
            reliable_split_buffer: Vec::new(),
            reliable_split_channel: None,
            recv_buffer: allocate_buffer(),
            send_buffer: allocate_buffer(),
            ack_sender,
            unreliable_split_shards: VecDeque::with_capacity(UNRELIABLE_BUFFERS),
            unreliable_split_buffer: Vec::new(),
        };

        let sender = Sender {
            unreliable: UnreliableSender {
                shared: shared.clone(),
                unreliable_split_id: 0,
            },
            reliable: ReliableSender {
                shared,
                queue_front_sequence: 0,
                queue: VecDeque::new(),
                ack_receiver,
            },
        };

        Ok(Connection {
            self_key,
            peer_key,
            sender,
            receiver,
        })
    }
}

struct Shared {
    id: Id,
    cipher: ChaCha20Poly1305,
    transport: UdpSocket,
}

impl Drop for Shared {
    fn drop(&mut self) {
        let mut buffer = ZEROED_BUFFER;

        let mut write_cursor = Cursor::new(buffer.as_mut_slice());

        write_cursor.write_varint(self.id).unwrap();
        write_cursor.write_varint(Type::DISCONNECT).unwrap();

        let tag_start = write_cursor.position();

        let len = crate::tag_sign_in_buffer(&mut buffer, &self.cipher, tag_start as usize);

        let _ = self.transport.try_send(&buffer[0 .. len]);
    }
}

#[derive(Clone)]
struct QueueEntry {
    // None means it's in the Receiver::recv_buffer
    buffer: Option<BoxBuffer>,
    start: usize,
    stop: usize,
    channel: Channel,
    is_split: bool,
}

/// Received message with metadata.
pub struct ReceivedData<'a> {
    channel: Channel,
    is_reliable: bool,
    data: &'a [u8],
}

impl ReceivedData<'_> {
    /// Get channel of this message.
    pub fn channel(&self) -> Channel {
        self.channel
    }

    /// If this data was sent as a reliable or unreliable message.
    pub fn is_reliable(&self) -> bool {
        self.is_reliable
    }

    /// Access message data.
    pub fn data(&self) -> &[u8] {
        self.data
    }
}

/// Message-receiving part of the connection.
pub struct Receiver {
    shared: Rc<Shared>,
    sequence: Sequence,
    reliable_queue: VecDeque<Option<QueueEntry>>,
    reliable_split_buffer: Vec<u8>,
    reliable_split_channel: Option<Channel>,
    recv_buffer: BoxBuffer,
    send_buffer: BoxBuffer,
    ack_sender: ChannelTx<Sequence>,
    unreliable_split_shards: VecDeque<UnreliableBuffer>,
    unreliable_split_buffer: Vec<u8>,
}

impl Receiver {
    /// Receive a message.
    ///
    /// **Futures returned must be constantly polled in loop in order to send reliable messages
    /// using `Sender`!**
    pub async fn recv(&mut self) -> Result<ReceivedData, Error> {
        loop {
            // Firstly, we check the reliable message queue
            // in case we have messages ready, handle these first
            if self
                .reliable_queue
                .front()
                .and_then(|f| f.as_ref())
                .is_some()
            {
                let QueueEntry {
                    buffer: queue_buffer_box,
                    start,
                    stop,
                    channel,
                    is_split,
                } = self.reliable_queue.pop_front().flatten().unwrap();

                self.reliable_queue.push_back(None);
                self.sequence = self.sequence.wrapping_add(1);

                // None means it's in the recv_buffer, we just received that packet in the previous
                // iteration of the loop
                let queue_buffer =
                    &queue_buffer_box.as_ref().unwrap_or(&self.recv_buffer)[start .. stop];

                if is_split {
                    // Split started, if not started - cleanup & start,
                    // if we already started - check the channel
                    if self.reliable_split_channel.is_none() {
                        self.reliable_split_channel = Some(channel);
                        self.reliable_split_buffer.clear();
                    } else if let Some(reliable_split_channel) = self.reliable_split_channel {
                        if reliable_split_channel != channel {
                            debug!("skipping mishappened packet with channel {}", channel);
                            continue;
                        }
                    }

                    if self.reliable_split_buffer.len() + queue_buffer.len() > MAX_SPLIT_DATA_SIZE {
                        return Err(Error::PeerMessageTooLarge);
                    }

                    self.reliable_split_buffer.extend_from_slice(queue_buffer);

                    continue;
                } else {
                    let buf = if let Some(reliable_split_channel) = self.reliable_split_channel {
                        // Split just completed, extending and returning
                        if reliable_split_channel == channel {
                            if self.reliable_split_buffer.len() + queue_buffer.len()
                                > MAX_SPLIT_DATA_SIZE
                            {
                                return Err(Error::PeerMessageTooLarge);
                            }

                            self.reliable_split_buffer.extend_from_slice(queue_buffer);

                            self.reliable_split_channel = None;

                            self.reliable_split_buffer.as_slice()
                        } else {
                            continue;
                        }
                    } else {
                        // Non-split packet arrived
                        if let Some(queue_buffer) = queue_buffer_box {
                            self.recv_buffer = queue_buffer;
                        }

                        &self.recv_buffer[start .. stop]
                    };

                    return Ok(ReceivedData {
                        channel,
                        is_reliable: true,
                        data: buf,
                    });
                }
            }

            let len = self
                .shared
                .transport
                .recv(self.recv_buffer.as_mut())
                .await?;

            let mut read_cursor = Cursor::new(&self.recv_buffer[.. len]);

            let sender: usize = seek_read!(read_cursor.read_varint(), "sender");

            if sender != SERVER_ID {
                continue;
            }

            let mut packet_type = Type::UNDEFINED;
            seek_read!(
                read_cursor.read_exact(slice::from_mut(&mut packet_type)),
                "type"
            );

            let tag_start = read_cursor.position() as usize;

            let decrypted_start = match crate::decode_in_buffer(
                &mut self.recv_buffer[.. len],
                tag_start,
                &self.shared.cipher,
            ) {
                Ok(s) => s,
                Err(()) => continue,
            };

            let mut read_cursor = Cursor::new(&self.recv_buffer[.. len]);
            read_cursor.set_position(decrypted_start as u64);

            match packet_type {
                Type::ACKNOWLEDGE => {
                    let sequence: Sequence = seek_read!(read_cursor.read_varint(), "sequence");
                    let _ = self.ack_sender.send(sequence);
                },
                Type::DISCONNECT => {
                    return Err(Error::Disconnect);
                },
                Type::UNRELIABLE => {
                    let channel: Channel = seek_read!(read_cursor.read_varint(), "channel");
                    let start = read_cursor.position() as usize;
                    return Ok(ReceivedData {
                        channel,
                        is_reliable: false,
                        data: &self.recv_buffer[start .. len],
                    });
                },
                Type::UNRELIABLE_SPLIT_START => {
                    let channel: Channel = seek_read!(read_cursor.read_varint(), "channel");
                    let split_id: u16 = seek_read!(read_cursor.read_varint(), "split_id");
                    let expected_packets: usize =
                        seek_read!(read_cursor.read_varint(), "expected_packets");

                    if expected_packets > MAX_SPLIT_PACKETS {
                        log::debug!(
                            "dropping packet with packet length {} more than maximum {}",
                            expected_packets,
                            MAX_SPLIT_PACKETS,
                        );
                        continue;
                    }

                    let mut split_buffer = if self.unreliable_split_shards.len()
                        == UNRELIABLE_BUFFERS
                        || self.unreliable_split_shards.back().is_some()
                            && self.unreliable_split_shards.back().unwrap().is_complete()
                    {
                        let mut b = self.unreliable_split_shards.pop_back().unwrap();
                        let UnreliableBuffer {
                            split_id: b_split_id,
                            channel: b_channel,
                            complete_shards: b_complete_shards,
                            shards: b_shards,
                        } = &mut b;
                        *b_split_id = split_id;
                        *b_channel = channel;
                        *b_complete_shards = 0;
                        b_shards.clear();
                        b_shards.resize(expected_packets, UnreliableBufferShard::new());
                        b
                    } else {
                        UnreliableBuffer {
                            split_id,
                            channel,
                            complete_shards: 0,
                            shards: vec![UnreliableBufferShard::new(); expected_packets],
                        }
                    };

                    let start = read_cursor.position() as usize;

                    let data_length = len - start;

                    let shard = split_buffer.shards.get_mut(0).unwrap();

                    shard.buffer[.. data_length].copy_from_slice(&self.recv_buffer[start .. len]);
                    shard.length = data_length;
                    shard.written = true;

                    split_buffer.complete_shards += 1;

                    self.unreliable_split_shards.push_front(split_buffer);
                },
                Type::UNRELIABLE_SPLIT => {
                    let channel: Channel = seek_read!(read_cursor.read_varint(), "channel");
                    let split_id: u16 = seek_read!(read_cursor.read_varint(), "split_id");
                    let count: usize = seek_read!(read_cursor.read_varint(), "count");
                    let start = read_cursor.position() as usize;
                    let data_length = len - start;

                    let split_buffer = match self.unreliable_split_shards.iter_mut().find(|b| {
                        b.split_id == split_id && b.channel == channel && !b.is_complete()
                    }) {
                        Some(b) => b,
                        None => {
                            debug!("split buffer not found for split {}", split_id);
                            continue;
                        },
                    };

                    let shard = match split_buffer.shards.get_mut(count) {
                        Some(s) => s,
                        None => {
                            debug!("shard not found for count {}", count);
                            continue;
                        },
                    };

                    if shard.written {
                        debug!("shard is already written for count {}", count);
                        continue;
                    }

                    shard.buffer[.. data_length].copy_from_slice(&self.recv_buffer[start .. len]);
                    shard.length = data_length;
                    shard.written = true;

                    split_buffer.complete_shards += 1;

                    if split_buffer.is_complete() {
                        self.unreliable_split_buffer.clear();

                        for shard in split_buffer.shards.iter() {
                            self.unreliable_split_buffer
                                .extend_from_slice(&shard.buffer[.. shard.length]);
                        }

                        // TODO: also check CRC and if it's incorrect restore buf length to
                        // MAX_PACKET_SIZE before continuing

                        return Ok(ReceivedData {
                            channel,
                            is_reliable: false,
                            data: self.unreliable_split_buffer.as_slice(),
                        });
                    }
                },
                Type::RELIABLE | Type::RELIABLE_SPLIT => {
                    let channel: Channel = seek_read!(read_cursor.read_varint(), "channel");
                    let sequence: Sequence = seek_read!(read_cursor.read_varint(), "sequence");

                    let start = read_cursor.position() as usize;
                    // TODO verify correctness
                    let index = sequence.wrapping_sub(self.sequence);
                    if index < RELIABLE_QUEUE_LENGTH {
                        let queue_place = self.reliable_queue.get_mut(index as usize).unwrap();

                        *queue_place = Some(QueueEntry {
                            buffer: if index == 0 {
                                // Ready to give that message right away in the next loop
                                // iteration, we don't want to allocate buffer to drop it right after
                                None
                            } else {
                                Some(mem::replace(&mut self.recv_buffer, allocate_buffer()))
                            },
                            start,
                            stop: len,
                            channel,
                            is_split: packet_type == Type::RELIABLE_SPLIT,
                        });
                    }

                    // TODO: do not answer if the sequence is not previous, but random?
                    seek_write!(
                        send_ack(&mut self.send_buffer, self.shared.as_ref(), sequence).await,
                        "ack message"
                    );
                },
                _ => {},
            }
        }
    }
}

/// Message-sending part of the connection. Contains both reliable-sending and unreliable-sending
/// halves, therefore can send both types. Can be `split()` to have those halves separate.
pub struct Sender {
    unreliable: UnreliableSender,
    reliable: ReliableSender,
}

impl Sender {
    /// Send a data slice unreliably.
    pub async fn send_unreliable(&mut self, channel: Channel, data: &[u8]) -> Result<(), Error> {
        self.unreliable.send_unreliable(channel, data).await
    }

    /// Send a data slice reliably.
    ///
    /// **Lazily sends previous undelivered reliable messages before trying to send a new one.
    /// It is highly recommended to send keepalive packets periodically to have lost messages retransmitted.**
    pub async fn send_reliable(&mut self, channel: Channel, data: &[u8]) -> Result<(), Error> {
        self.reliable.send_reliable(channel, data).await
    }

    /// Wait for all transmitted data to be delivered.
    /// Resends lost messages periodically internally.
    pub async fn wait_complete(&mut self) -> Result<(), Error> {
        self.reliable.wait_complete().await
    }

    /// Split the `Sender` into `ReliableSender` and `UnreliableSender` halves.
    pub fn split(self) -> (UnreliableSender, ReliableSender) {
        let Self {
            unreliable,
            reliable,
        } = self;

        (unreliable, reliable)
    }
}

/// Unreliable-sending part of the connection.
pub struct UnreliableSender {
    shared: Rc<Shared>,
    unreliable_split_id: u16,
}

impl UnreliableSender {
    async fn send_unreliable_one(
        &self,
        channel: Channel,
        data: &[u8],
        message_type: u8,
        len_or_count: Option<usize>,
    ) -> Result<(), Error> {
        let mut buffer = ZEROED_BUFFER;

        let (tag_start, len) =
            crate::write_in_buffer(&mut buffer, self.shared.id, message_type, |cursor| {
                cursor.write_varint(channel).unwrap();
                if let Some(len_or_count) = len_or_count {
                    cursor.write_varint(self.unreliable_split_id).unwrap();
                    cursor.write_varint(len_or_count).unwrap();
                }
                cursor.write_all(data).unwrap();
            });

        crate::encode_in_buffer(&mut buffer, &self.shared.cipher, tag_start, len);

        self.shared.transport.send(&buffer[.. len]).await?;

        Ok(())
    }

    /// Send a data slice unreliably.
    pub async fn send_unreliable(&mut self, channel: Channel, data: &[u8]) -> Result<(), Error> {
        if data.len() > MAX_DATA_SIZE {
            self.unreliable_split_id = self.unreliable_split_id.wrapping_add(1);
            let length = data.len() / MAX_DATA_SIZE + 1;
            self.send_unreliable_one(
                channel,
                &data[0 .. MAX_DATA_SIZE],
                Type::UNRELIABLE_SPLIT_START,
                Some(length),
            )
            .await?;

            let mut start = MAX_DATA_SIZE;
            for count in 1 .. length {
                let stop = start + (data.len() - start).min(MAX_DATA_SIZE);
                self.send_unreliable_one(
                    channel,
                    &data[start .. stop],
                    Type::UNRELIABLE_SPLIT,
                    Some(count),
                )
                .await?;
                start = stop;
            }

            Ok(())
        } else {
            self.send_unreliable_one(channel, data, Type::UNRELIABLE, None)
                .await
        }
    }
}

enum PacketState {
    Done,
    Pending {
        sent_at: Instant,
        buffer: BoxBuffer,
        length: usize,
    },
}

/// Reliable-sending part of the connection.
pub struct ReliableSender {
    shared: Rc<Shared>,
    queue_front_sequence: Sequence,
    queue: VecDeque<PacketState>,
    ack_receiver: ChannelRx<Sequence>,
}

impl ReliableSender {
    fn pack_data(&mut self, channel: Channel, data: &[u8], packet_type: u8) -> (BoxBuffer, usize) {
        let mut buffer = allocate_buffer();

        let (tag_start, len) =
            crate::write_in_buffer(buffer.as_mut(), self.shared.id, packet_type, |cursor| {
                cursor.write_varint(channel).unwrap();
                cursor
                    .write_varint(
                        self.queue_front_sequence
                            .wrapping_add(self.queue.len() as u16),
                    )
                    .unwrap();
                cursor.write_all(data).unwrap();
            });

        crate::encode_in_buffer(buffer.as_mut(), &self.shared.cipher, tag_start, len);

        (buffer, len)
    }

    async fn handle_acks_resend(&mut self, mut must_wait: bool) -> Result<(), Error> {
        loop {
            // Handling previous ACKs first
            let ack = if must_wait {
                must_wait = false;
                let result = match time::timeout(RELIABLE_RESEND_AFTER, {
                    #[cfg(feature = "single")]
                    {
                        self.ack_receiver.recv()
                    }
                    #[cfg(feature = "multi")]
                    {
                        self.ack_receiver.recv_async()
                    }
                })
                .await
                {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                result.map_err(|_| Error::ReceiverWasDropped)?
            } else {
                match self.ack_receiver.try_recv() {
                    Ok(a) => a,
                    #[cfg(feature = "single")]
                    Err(TryReceiveError::Closed) => return Err(Error::ReceiverWasDropped),
                    #[cfg(feature = "multi")]
                    Err(TryReceiveError::Disconnected) => return Err(Error::ReceiverWasDropped),
                    Err(TryReceiveError::Empty) => break,
                }
            };

            let index = ack.wrapping_sub(self.queue_front_sequence);

            if index < RELIABLE_QUEUE_LENGTH {
                if let Some(queue_entry) = self.queue.get_mut(index as usize) {
                    *queue_entry = PacketState::Done;
                }
            }
        }

        // Getting rid of confirmed packets
        while matches!(self.queue.front(), Some(PacketState::Done)) {
            self.queue.pop_front();
            self.queue_front_sequence = self.queue_front_sequence.wrapping_add(1);
        }

        let mut queue = mem::take(&mut self.queue);

        // Lazily resending lost packages
        for (sent_at, buffer, length) in queue.iter_mut().filter_map(|entry| {
            match entry {
                PacketState::Pending {
                    sent_at,
                    buffer,
                    length,
                } => Some((sent_at, buffer, length)),
                PacketState::Done => None,
            }
        }) {
            if sent_at.elapsed() > RELIABLE_RESEND_AFTER {
                if let Err(err) = self.shared.transport.send(&buffer[.. *length]).await {
                    self.queue = queue;
                    return Err(err.into());
                }
                *sent_at = Instant::now();
            }
        }

        self.queue = queue;

        Ok(())
    }

    async fn send_reliable_one(
        &mut self,
        channel: Channel,
        data: &[u8],
        packet_type: u8,
    ) -> Result<(), Error> {
        let mut must_wait = false;
        loop {
            self.handle_acks_resend(mem::replace(&mut must_wait, false))
                .await?;

            if matches!(self.queue.front(), Some(PacketState::Pending { .. }))
                && self.queue.len() >= RELIABLE_QUEUE_LENGTH as usize
            {
                // Waiting list is full
                must_wait = true;
                continue;
            } else {
                // Finally send our latest packet and add that to waiting list
                let (buffer, length) = self.pack_data(channel, data, packet_type);
                let result = self.shared.transport.send(&buffer[.. length]).await;
                self.queue.push_back(PacketState::Pending {
                    sent_at: Instant::now(),
                    buffer,
                    length,
                });

                result?;

                return Ok(());
            }
        }
    }

    /// Send a data slice reliably.
    ///
    /// **Lazily sends previous undelivered reliable messages before trying to send a new one.
    /// It is highly recommended to send keepalive packets periodically to have lost messages retransmitted.**
    pub async fn send_reliable(&mut self, channel: Channel, data: &[u8]) -> Result<(), Error> {
        let mut start = 0;

        while data.len() - start > MAX_DATA_SIZE {
            self.send_reliable_one(
                channel,
                &data[start .. start + MAX_DATA_SIZE],
                Type::RELIABLE_SPLIT,
            )
            .await?;

            start += MAX_DATA_SIZE;
        }

        self.send_reliable_one(channel, &data[start ..], Type::RELIABLE)
            .await?;

        Ok(())
    }

    /// Wait for all transmitted data to be delivered.
    /// Resends lost messages periodically internally.
    pub async fn wait_complete(&mut self) -> Result<(), Error> {
        while !self.queue.is_empty() {
            self.handle_acks_resend(true).await?;
        }

        Ok(())
    }
}
