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
//!         sender.send_reliable(b"Hello Server!").await;
//!
//!         // Senders send no data passively by themselves and resending lost messages
//!         // in reliable data transfer happens lazily, right before sending a new one.
//!         // To make sure all messages are delivered, there must be either a periodical
//!         // "keepalive" message or a [`wait_complete`] call.
//!         // [`wait_complete`] resends lost messages and only completes when all messages
//!         // are delivered.
//!         sender.wait_complete().await;
//!     };
//!
//!     future::or(recv_future, send_future).await;
//! }
//! ```

use crate::{
    seek_read,
    AsSlice,
    Id,
    Key,
    ReadExt,
    Sequence,
    ToU128,
    ToUsize,
    Type,
    UnreliableBuffer,
    WriteExt,
    ENCRYPTED_START,
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

type BoxBuffer = Box<Buffer>;

fn allocate_buffer() -> BoxBuffer {
    let buf = Box::new_uninit();

    unsafe { buf.assume_init() }
}

async fn send_ack(buffer: &mut Buffer, shared: &Shared, sequence: Sequence) -> Result<(), Error> {
    let len = crate::write_in_buffer(buffer, shared.id, Type::ACKNOWLEDGE, sequence, |_| {});

    crate::encode_in_buffer(buffer, &shared.cipher, len);

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
    /// Connection is invalid and must be dropped and possibly restarted. Must not happen in
    /// practice.
    InvalidConnection,
    /// Peer sent message that is too large.
    PeerMessageTooLarge,
    /// Message is too large to be sent.
    MessageTooLarge,
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

        let mut buf = allocate_buffer();

        let mut write_cursor = Cursor::new(buf.as_mut_slice());

        write_cursor.write_bytes(NEW_CONNECTION_ID).unwrap();
        write_cursor.write_bytes(Type::CONNECT).unwrap();
        write_cursor.write_all(&self_key).unwrap();

        transport.send(write_cursor.slice()).await?;

        let (peer_key, deciphered_peer_key, id) = loop {
            let len = transport.recv(buf.as_mut_slice()).await?;

            let mut read_cursor = Cursor::new(&buf[.. len]);

            let sender: Id = seek_read!(read_cursor.read_bytes(), "sender");

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
                let id: Id = seek_read!(read_cursor.read_bytes(), "id");

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
            reliable_split_ongoing: false,
            recv_buffer: allocate_buffer(),
            send_buffer: allocate_buffer(),
            ack_sender,
            unreliable_split_buffers: VecDeque::with_capacity(UNRELIABLE_BUFFERS),
            unreliable_split_buffer: Vec::new(),
        };

        let sender = Sender {
            unreliable: UnreliableSender {
                shared: shared.clone(),
                buffer: allocate_buffer(),
                sequence: 0,
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
        let mut buffer = allocate_buffer();

        let mut write_cursor = Cursor::new(buffer.as_mut_slice());

        write_cursor.write_bytes(self.id).unwrap();
        write_cursor.write_bytes(Type::DISCONNECT).unwrap();

        crate::tag_sign_in_buffer(&mut buffer, &self.cipher);

        let _ = self.transport.try_send(&buffer[0 .. ENCRYPTED_START]);
    }
}

#[derive(Clone)]
struct QueueEntry {
    // None means it's in the Receiver::recv_buffer
    buffer: Option<BoxBuffer>,
    start: usize,
    stop: usize,
    is_split: bool,
}

/// Received message with metadata.
pub struct ReceivedData<'a> {
    is_reliable: bool,
    data: &'a [u8],
}

impl ReceivedData<'_> {
    /// If this data was sent as a reliable or unreliable message.
    pub fn is_reliable(&self) -> bool {
        self.is_reliable
    }

    /// Access message data.
    pub fn data(&self) -> &[u8] {
        self.data
    }
}

struct LimitedBoxBuffer {
    buffer: BoxBuffer,
    start: usize,
    stop: usize,
}

impl AsRef<[u8]> for LimitedBoxBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[self.start .. self.stop]
    }
}

/// Message-receiving part of the connection.
pub struct Receiver {
    shared: Rc<Shared>,
    sequence: Sequence,
    reliable_queue: VecDeque<Option<QueueEntry>>,
    reliable_split_buffer: Vec<u8>,
    reliable_split_ongoing: bool,
    recv_buffer: BoxBuffer,
    send_buffer: BoxBuffer,
    ack_sender: ChannelTx<Sequence>,
    unreliable_split_buffers: VecDeque<UnreliableBuffer<LimitedBoxBuffer>>,
    unreliable_split_buffer: Vec<u8>,
}

impl Receiver {
    /// Receive a message.
    ///
    /// **Futures returned must be constantly polled in loop in order to send reliable messages
    /// using `Sender`!**
    pub async fn recv(&mut self) -> Result<ReceivedData<'_>, Error> {
        loop {
            // Firstly, we check the reliable message queue
            // in case we have messages ready, handle these first
            if self
                .reliable_queue
                .front()
                .and_then(|f| f.as_ref())
                .is_some()
            {
                // First try to increment to prevent theoretical overflow:
                self.sequence = self
                    .sequence
                    .checked_add(1)
                    .ok_or(Error::InvalidConnection)?;

                let QueueEntry {
                    buffer: queue_buffer_box,
                    start,
                    stop,
                    is_split,
                } = self.reliable_queue.pop_front().flatten().unwrap();

                self.reliable_queue.push_back(None);

                // None means it's in the recv_buffer, we just received that packet in the previous
                // iteration of the loop
                let queue_buffer =
                    &queue_buffer_box.as_ref().unwrap_or(&self.recv_buffer)[start .. stop];

                if is_split {
                    // Split started, if not started - cleanup & start
                    if !self.reliable_split_ongoing {
                        self.reliable_split_ongoing = true;
                        self.reliable_split_buffer.clear();
                    }

                    if self.reliable_split_buffer.len() + queue_buffer.len() > MAX_SPLIT_DATA_SIZE {
                        return Err(Error::PeerMessageTooLarge);
                    }

                    self.reliable_split_buffer.extend_from_slice(queue_buffer);

                    continue;
                } else {
                    let buf = if self.reliable_split_ongoing {
                        // Split just completed, extending and returning
                        if self.reliable_split_buffer.len() + queue_buffer.len()
                            > MAX_SPLIT_DATA_SIZE
                        {
                            return Err(Error::PeerMessageTooLarge);
                        }

                        self.reliable_split_buffer.extend_from_slice(queue_buffer);

                        self.reliable_split_ongoing = false;

                        self.reliable_split_buffer.as_slice()
                    } else {
                        // Non-split packet arrived
                        if let Some(queue_buffer) = queue_buffer_box {
                            self.recv_buffer = queue_buffer;
                        }

                        &self.recv_buffer[start .. stop]
                    };

                    return Ok(ReceivedData {
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

            let sender: Id = seek_read!(read_cursor.read_bytes(), "sender");

            if sender != SERVER_ID {
                continue;
            }

            let packet_type: u8 = seek_read!(read_cursor.read_bytes(), "type");

            if crate::decode_in_buffer(&mut self.recv_buffer[.. len], &self.shared.cipher).is_err()
            {
                continue;
            };

            if packet_type == Type::DISCONNECT {
                return Err(Error::Disconnect);
            }

            let mut read_cursor = Cursor::new(&self.recv_buffer[.. len]);
            read_cursor.set_position(ENCRYPTED_START as u64);

            let sequence: Sequence = seek_read!(read_cursor.read_bytes(), "sequence");

            match packet_type {
                Type::ACKNOWLEDGE => {
                    let _ = self.ack_sender.send(sequence);
                },
                Type::UNRELIABLE => {
                    let start = read_cursor.position().to_usize();
                    return Ok(ReceivedData {
                        is_reliable: false,
                        data: &self.recv_buffer[start .. len],
                    });
                },
                Type::UNRELIABLE_SPLIT_START => {
                    let expected_packets: u32 =
                        seek_read!(read_cursor.read_bytes(), "expected_packets");

                    if sequence.checked_add(expected_packets.to_u128()).is_none() {
                        log::debug!("dropping uncompletable split packet");
                        continue;
                    }

                    if expected_packets > MAX_SPLIT_PACKETS {
                        log::debug!(
                            "dropping packet with packet length {} more than maximum {}",
                            expected_packets,
                            MAX_SPLIT_PACKETS,
                        );
                        continue;
                    }

                    let complete_index = self
                        .unreliable_split_buffers
                        .iter()
                        .enumerate()
                        .find(|(_, b)| b.is_complete())
                        .map(|(i, _)| i);

                    let mut split_buffer = if let Some(index) = complete_index {
                        self.unreliable_split_buffers.remove(index).unwrap()
                    } else if self.unreliable_split_buffers.len() < UNRELIABLE_BUFFERS {
                        UnreliableBuffer::new(sequence, expected_packets)
                    } else {
                        let (index, min_start_seq) = self
                            .unreliable_split_buffers
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, b)| b.start_sequence)
                            .map(|(i, b)| (i, b.start_sequence))
                            .unwrap();

                        if min_start_seq >= sequence {
                            continue;
                        }

                        self.unreliable_split_buffers.remove(index).unwrap()
                    };

                    split_buffer.clear(sequence, expected_packets);

                    let start = read_cursor.position().to_usize();

                    let shard = split_buffer.shards.get_mut(0).unwrap();

                    *shard = Some(LimitedBoxBuffer {
                        buffer: mem::replace(&mut self.recv_buffer, allocate_buffer()),
                        start,
                        stop: len,
                    });

                    split_buffer.complete_shards += 1;

                    self.unreliable_split_buffers.push_front(split_buffer);
                },
                Type::UNRELIABLE_SPLIT => {
                    let start = read_cursor.position().to_usize();

                    let Some(split_buffer) = self
                        .unreliable_split_buffers
                        .iter_mut()
                        .find(|b| {
                            sequence > b.start_sequence
                                // Correctness: boundaries must be checked in
                                // UNRELIABLE_SPLIT_START handler
                                && sequence < b.start_sequence.checked_add(b.shards.len().to_u128()).unwrap()
                                && !b.is_complete()
                        }) else { continue };

                    let count = (sequence - split_buffer.start_sequence).to_usize();

                    let shard = split_buffer.shards.get_mut(count).unwrap();

                    if shard.is_some() {
                        debug!("shard is already written for count {}", count);
                        continue;
                    }

                    *shard = Some(LimitedBoxBuffer {
                        buffer: mem::replace(&mut self.recv_buffer, allocate_buffer()),
                        start,
                        stop: len,
                    });

                    split_buffer.complete_shards += 1;

                    if split_buffer.is_complete() {
                        self.unreliable_split_buffer.clear();

                        for shard in split_buffer.shards.iter() {
                            self.unreliable_split_buffer
                                .extend_from_slice(shard.as_ref().unwrap().as_ref());
                        }

                        // TODO: also check CRC and if it's incorrect restore buf length to
                        // MAX_PACKET_SIZE before continuing

                        return Ok(ReceivedData {
                            is_reliable: false,
                            data: self.unreliable_split_buffer.as_slice(),
                        });
                    }
                },
                Type::RELIABLE | Type::RELIABLE_SPLIT => {
                    if let Some(index) = sequence.checked_sub(self.sequence) {
                        let start = read_cursor.position().to_usize();

                        if index < RELIABLE_QUEUE_LENGTH {
                            let queue_place =
                                self.reliable_queue.get_mut(index.to_usize()).unwrap();

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
                                is_split: packet_type == Type::RELIABLE_SPLIT,
                            });
                        }
                    }

                    if sequence.abs_diff(self.sequence) <= RELIABLE_QUEUE_LENGTH {
                        send_ack(&mut self.send_buffer, self.shared.as_ref(), sequence).await?;
                    }
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
    pub async fn send_unreliable(&mut self, data: &[u8]) -> Result<(), Error> {
        self.unreliable.send_unreliable(data).await
    }

    /// Send a data slice reliably.
    ///
    /// **Lazily sends previous undelivered reliable messages before trying to send a new one.
    /// It is highly recommended to send keepalive packets periodically to have lost messages retransmitted.**
    pub async fn send_reliable(&mut self, data: &[u8]) -> Result<(), Error> {
        self.reliable.send_reliable(data).await
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
    buffer: BoxBuffer,
    sequence: Sequence,
}

impl UnreliableSender {
    async fn send_unreliable_one(
        &mut self,
        message_type: u8,
        len: Option<u32>,
        data: &[u8],
    ) -> Result<(), Error> {
        let sequence = {
            let prev = self.sequence;
            self.sequence = self
                .sequence
                .checked_add(1)
                .ok_or(Error::InvalidConnection)?;
            prev
        };

        let len = crate::write_in_buffer(
            &mut self.buffer,
            self.shared.id,
            message_type,
            sequence,
            |cursor| {
                if let Some(len) = len {
                    cursor.write_bytes(len).unwrap();
                }
                cursor.write_all(data).unwrap();
            },
        );

        crate::encode_in_buffer(&mut self.buffer, &self.shared.cipher, len);

        self.shared.transport.send(&self.buffer[.. len]).await?;

        Ok(())
    }

    /// Send a data slice unreliably.
    pub async fn send_unreliable(&mut self, data: &[u8]) -> Result<(), Error> {
        if data.len() > MAX_DATA_SIZE {
            let length = (data.len() / MAX_DATA_SIZE + 1)
                .try_into()
                .map_err(|_| Error::MessageTooLarge)?;
            self.send_unreliable_one(
                Type::UNRELIABLE_SPLIT_START,
                Some(length),
                &data[0 .. MAX_DATA_SIZE],
            )
            .await?;

            let mut start = MAX_DATA_SIZE;
            for _ in 1 .. length {
                let stop = start + (data.len() - start).min(MAX_DATA_SIZE);
                self.send_unreliable_one(Type::UNRELIABLE_SPLIT, None, &data[start .. stop])
                    .await?;
                start = stop;
            }

            Ok(())
        } else {
            self.send_unreliable_one(Type::UNRELIABLE, None, data).await
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
    fn pack_data(
        &mut self,
        packet_type: u8,
        sequence: Sequence,
        data: &[u8],
    ) -> (BoxBuffer, usize) {
        let mut buffer = allocate_buffer();

        let len = crate::write_in_buffer(
            buffer.as_mut(),
            self.shared.id,
            packet_type,
            sequence,
            |cursor| {
                cursor.write_all(data).unwrap();
            },
        );

        crate::encode_in_buffer(buffer.as_mut(), &self.shared.cipher, len);

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

            let Some(index) = ack.checked_sub(self.queue_front_sequence) else {
                continue;
            };

            if index < RELIABLE_QUEUE_LENGTH {
                if let Some(queue_entry) = self.queue.get_mut(index.to_usize()) {
                    *queue_entry = PacketState::Done;
                }
            }
        }

        // Getting rid of confirmed packets
        while matches!(self.queue.front(), Some(PacketState::Done)) {
            self.queue.pop_front();
            // Correctness: adding into the back of the queue must be checked:
            self.queue_front_sequence += 1;
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

    async fn send_reliable_one(&mut self, packet_type: u8, data: &[u8]) -> Result<(), Error> {
        let mut must_wait = false;
        loop {
            self.handle_acks_resend(mem::replace(&mut must_wait, false))
                .await?;

            if matches!(self.queue.front(), Some(PacketState::Pending { .. }))
                && self.queue.len() >= const { RELIABLE_QUEUE_LENGTH as usize }
            {
                // Waiting list is full
                must_wait = true;
                continue;
            } else {
                let sequence = self
                    .queue_front_sequence
                    .checked_add(self.queue.len().to_u128())
                    .ok_or(Error::InvalidConnection)?;
                // Finally send our latest packet and add that to waiting list
                let (buffer, length) = self.pack_data(packet_type, sequence, data);
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
    pub async fn send_reliable(&mut self, data: &[u8]) -> Result<(), Error> {
        let mut start = 0;

        while data.len() - start > MAX_DATA_SIZE {
            self.send_reliable_one(Type::RELIABLE_SPLIT, &data[start .. start + MAX_DATA_SIZE])
                .await?;

            start += MAX_DATA_SIZE;
        }

        self.send_reliable_one(Type::RELIABLE, &data[start ..])
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
