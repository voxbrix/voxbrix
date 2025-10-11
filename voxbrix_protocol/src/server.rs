//! Server side of the protocol implementation.
//!
//! # Examples
//!
//! ```no_run
//! use futures_lite::future::{
//!     self,
//!     FutureExt,
//! };
//! use std::time::Duration;
//! use tokio::time;
//! use voxbrix_protocol::server::{
//!     Connection,
//!     ServerParameters,
//! };
//!
//! async fn example() {
//!     let mut server = ServerParameters::default()
//!         .bind(([127, 0, 0, 1], 12345))
//!         .await
//!         .expect("socket bound");
//!
//!     let Connection {
//!         mut receiver,
//!         mut sender,
//!         ..
//!     } = server.accept().await.expect("accepted a connection");
//!
//!     let recv_future = async {
//!         while let Ok(msg) = receiver.recv().await {
//!             println!(
//!                 "channel: {}, data: {:?}",
//!                 msg.channel(),
//!                 msg.data().as_ref()
//!             );
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
//!     let server_future = async {
//!         // For above connections to work, the future from accept() method must always be
//!         // polled in loop, even if you do not actually use the incoming connections.
//!         while let Ok(_conn) = server.accept().await {
//!             // Serve the new connection
//!         }
//!     };
//!
//!     server_future.or(recv_future.or(send_future)).await;
//! }
//! ```
use crate::{
    seek_read,
    AsSlice,
    Channel,
    Id,
    Key,
    ReadExt,
    Sequence,
    SplitId,
    ToU128,
    ToUsize,
    Type,
    UnreliableBuffer,
    UnreliableBufferShard,
    WriteExt,
    ENCRYPTION_START,
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
    TYPE_INDEX,
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
    future::Future,
    io::{
        Cursor,
        Error as StdIoError,
        Read,
        Write,
    },
    mem,
    net::SocketAddr,
    pin::pin,
    slice,
    task::Poll,
    time::Instant,
};
use tokio::{
    net::UdpSocket,
    time,
};

pub const DEFAULT_MAX_CONNECTIONS: Id = 64;

// NOT cloneable
struct WriteBuffer(Rc<[u8; MAX_PACKET_SIZE]>);

impl WriteBuffer {
    fn new() -> Self {
        let buf = Rc::new_uninit();
        let buf = unsafe { buf.assume_init() };
        WriteBuffer(buf)
    }

    fn finish(self, start: usize, stop: usize) -> ReadBuffer {
        ReadBuffer {
            buffer: self.0,
            start,
            stop,
        }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        Rc::get_mut(&mut self.0).unwrap()
    }
}

impl AsRef<[u8]> for WriteBuffer {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsMut<[u8; MAX_PACKET_SIZE]> for WriteBuffer {
    fn as_mut(&mut self) -> &mut [u8; MAX_PACKET_SIZE] {
        Rc::get_mut(&mut self.0).unwrap()
    }
}

#[derive(Clone)]
struct ReadBuffer {
    // Avoid bloating enums that use Packet
    // Allows cheap cloning
    buffer: Rc<[u8; MAX_PACKET_SIZE]>,
    start: usize,
    stop: usize,
}

impl AsRef<[u8]> for ReadBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[self.start .. self.stop]
    }
}

#[derive(Debug)]
pub enum Error {
    /// IO error wrapper.
    Io(StdIoError),
    /// Returned on attempt to send reliable message with the `Server` dropped.
    ServerWasDropped,
    /// Returned by the receiver in case the client dropped the connection handles.
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

/// Represents a data slice received. Implements `AsMut<[u8]>` and `AsRef<[u8]>`.
pub struct Packet {
    data: Data,
}

impl From<ReadBuffer> for Packet {
    fn from(from: ReadBuffer) -> Self {
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
    Single(ReadBuffer),
}

impl AsRef<[u8]> for Packet {
    fn as_ref(&self) -> &[u8] {
        match &self.data {
            Data::Collection(v) => v.as_ref(),
            Data::Single(a) => a.as_ref(),
        }
    }
}

struct InBuffer {
    buffer: WriteBuffer,
    stop: usize,
}

enum Out {
    Buffer {
        peer: Id,
        buffer: ReadBuffer,
        result_tx: FeedbackSender,
    },
    DropClient {
        peer: Id,
        cipher: ChaCha20Poly1305,
    },
}

struct FeedbackSender {
    seq: u8,
    tx: ChannelTx<(u8, Result<(), Error>)>,
}

impl FeedbackSender {
    fn send(self, msg: Result<(), Error>) -> Result<(), ()> {
        let Self { seq, tx } = self;

        #[cfg(feature = "single")]
        tx.send((seq, msg)).map_err(|_| ())?;
        #[cfg(feature = "multi")]
        tx.try_send((seq, msg)).map_err(|_| ())?;

        Ok(())
    }
}

// Channel for receiving IO feedback from the server task in response to sending a message
struct Feedback {
    seq: u8,
    tx: ChannelTx<(u8, Result<(), Error>)>,
    rx: ChannelRx<(u8, Result<(), Error>)>,
}

impl Feedback {
    fn new() -> Self {
        let (tx, rx) = new_channel();

        Self { seq: 0, tx, rx }
    }

    fn new_sender(&mut self) -> FeedbackSender {
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);

        FeedbackSender {
            seq,
            tx: self.tx.clone(),
        }
    }

    async fn receive(&mut self) -> Result<(), Error> {
        let expect_seq = self.seq.wrapping_sub(1);

        loop {
            let (seq, msg) = {
                #[cfg(feature = "single")]
                {
                    self.rx.recv()
                }
                #[cfg(feature = "multi")]
                {
                    self.rx.recv_async()
                }
            }
            .await
            .map_err(|_| Error::ServerWasDropped)?;

            if seq == expect_seq {
                break msg;
            }
        }
    }
}

async fn stream_send_ack(
    shared: &Shared,
    feedback: &mut Feedback,
    sequence: Sequence,
) -> Result<(), Error> {
    let mut buffer = WriteBuffer::new();

    let stop = crate::write_in_buffer(buffer.as_mut(), SERVER_ID, Type::ACKNOWLEDGE, |cursor| {
        cursor.write_bytes(sequence).unwrap();
    });

    crate::encode_in_buffer(buffer.as_mut(), &shared.cipher, stop);

    shared
        .transport_sender
        .send(Out::Buffer {
            peer: shared.peer,
            buffer: buffer.finish(0, stop),
            result_tx: feedback.new_sender(),
        })
        .map_err(|_| Error::ServerWasDropped)?;

    feedback.receive().await?;

    Ok(())
}

struct Shared {
    peer: Id,
    cipher: ChaCha20Poly1305,
    transport_sender: ChannelTx<Out>,
}

impl Drop for Shared {
    fn drop(&mut self) {
        let _ = self.transport_sender.send(Out::DropClient {
            peer: self.peer,
            cipher: self.cipher.clone(),
        });
    }
}

/// Message-sending part of the connection. Contains both reliable-sending and unreliable-sending
/// halves, therefore can send both types. Can be `split()` to have those halves separate.
pub struct StreamSender {
    unreliable: StreamUnreliableSender,
    reliable: StreamReliableSender,
}

impl StreamSender {
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

    /// Split the `StreamSender` into `StreamUnreliableSender` and `StreamReliableSender` halves.
    pub fn split(self) -> (StreamUnreliableSender, StreamReliableSender) {
        let Self {
            unreliable,
            reliable,
        } = self;

        (unreliable, reliable)
    }
}

/// Unreliable-sending part of the connection.
pub struct StreamUnreliableSender {
    shared: Rc<Shared>,
    unreliable_split_id: SplitId,
    feedback: Feedback,
}

impl StreamUnreliableSender {
    async fn send_unreliable_one(
        &mut self,
        channel: Channel,
        data: &[u8],
        packet_type: u8,
        len_or_count: Option<u32>,
    ) -> Result<(), Error> {
        let mut buffer = WriteBuffer::new();

        let stop = crate::write_in_buffer(buffer.as_mut(), SERVER_ID, packet_type, |cursor| {
            cursor.write_bytes(channel).unwrap();
            if let Some(len_or_count) = len_or_count {
                cursor.write_bytes(self.unreliable_split_id).unwrap();
                cursor.write_bytes(len_or_count).unwrap();
            }
            cursor.write_all(data).unwrap();
        });

        crate::encode_in_buffer(buffer.as_mut(), &self.shared.cipher, stop);

        self.shared
            .transport_sender
            .send(Out::Buffer {
                peer: self.shared.peer,
                buffer: buffer.finish(0, stop),
                result_tx: self.feedback.new_sender(),
            })
            .map_err(|_| Error::ServerWasDropped)?;

        self.feedback.receive().await?;

        Ok(())
    }

    /// Send a data slice unreliably.
    pub async fn send_unreliable(&mut self, channel: Channel, data: &[u8]) -> Result<(), Error> {
        if data.len() > MAX_DATA_SIZE {
            self.unreliable_split_id = self
                .unreliable_split_id
                .checked_add(1)
                .ok_or(Error::InvalidConnection)?;
            let length = (data.len() / MAX_DATA_SIZE + 1)
                .try_into()
                .map_err(|_| Error::MessageTooLarge)?;
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
        buffer: ReadBuffer,
    },
}

/// Reliable-sending part of the connection.
pub struct StreamReliableSender {
    shared: Rc<Shared>,
    queue_front_sequence: Sequence,
    queue: VecDeque<PacketState>,
    ack_receiver: ChannelRx<InBuffer>,
    feedback: Feedback,
}

impl StreamReliableSender {
    async fn send_buffer(&mut self, buffer: ReadBuffer) -> Result<(), Error> {
        self.shared
            .transport_sender
            .send(Out::Buffer {
                peer: self.shared.peer,
                buffer,
                result_tx: self.feedback.new_sender(),
            })
            .map_err(|_| Error::ServerWasDropped)?;

        self.feedback.receive().await?;

        Ok(())
    }

    fn pack_data(
        &mut self,
        packet_type: u8,
        sequence: Sequence,
        channel: Channel,
        data: &[u8],
    ) -> ReadBuffer {
        let mut buffer = WriteBuffer::new();

        let stop = crate::write_in_buffer(buffer.as_mut(), SERVER_ID, packet_type, |cursor| {
            cursor.write_bytes(channel).unwrap();
            cursor.write_bytes(sequence).unwrap();
            cursor.write_all(data).unwrap();
        });

        crate::encode_in_buffer(buffer.as_mut(), &self.shared.cipher, stop);

        buffer.finish(0, stop)
    }

    async fn handle_acks_resend(&mut self, mut must_wait: bool) -> Result<(), Error> {
        loop {
            // Handling previous ACKs first
            let InBuffer { mut buffer, stop } = if must_wait {
                must_wait = false;

                // TODO timeout retry limit?
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

                result.map_err(|_| Error::ServerWasDropped)?
            } else {
                match self.ack_receiver.try_recv() {
                    Ok(a) => a,
                    #[cfg(feature = "single")]
                    Err(TryReceiveError::Closed) => return Err(Error::ServerWasDropped),
                    #[cfg(feature = "multi")]
                    Err(TryReceiveError::Disconnected) => return Err(Error::ServerWasDropped),
                    Err(TryReceiveError::Empty) => break,
                }
            };

            if crate::decode_in_buffer(&mut buffer.as_mut()[.. stop], &self.shared.cipher).is_err()
            {
                continue;
            };

            let mut read_cursor = Cursor::new(&buffer.as_ref()[ENCRYPTION_START .. stop]);
            let ack: Sequence = seek_read!(read_cursor.read_bytes(), "sequence");

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
            // Correctness: pushing into the back of the queue must be checked
            self.queue_front_sequence += 1;
        }

        let mut queue = mem::take(&mut self.queue);

        // Lazily resending lost packages
        for (sent_at, buffer) in queue.iter_mut().filter_map(|entry| {
            match entry {
                PacketState::Pending { sent_at, buffer } => Some((sent_at, buffer)),
                PacketState::Done => None,
            }
        }) {
            if sent_at.elapsed() > RELIABLE_RESEND_AFTER {
                self.send_buffer(buffer.clone()).await?;
                *sent_at = Instant::now();
            }
        }

        self.queue = queue;

        Ok(())
    }

    async fn send_reliable_one(
        &mut self,
        packet_type: u8,
        channel: Channel,
        data: &[u8],
    ) -> Result<(), Error> {
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
                let buffer = self.pack_data(packet_type, sequence, channel, data);
                self.queue.push_back(PacketState::Pending {
                    sent_at: Instant::now(),
                    buffer: buffer.clone(),
                });
                self.send_buffer(buffer).await?;

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
            let stop = start + MAX_DATA_SIZE;
            self.send_reliable_one(Type::RELIABLE_SPLIT, channel, &data[start .. stop])
                .await?;

            start = stop;
        }

        self.send_reliable_one(Type::RELIABLE, channel, &data[start ..])
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

#[derive(Clone)]
struct QueueEntry {
    buffer: ReadBuffer,
    channel: Channel,
    is_split: bool,
}

/// Received message with metadata.
pub struct ReceivedData {
    channel: Channel,
    is_reliable: bool,
    data: Packet,
}

impl ReceivedData {
    /// Get channel of this message.
    pub fn channel(&self) -> Channel {
        self.channel
    }

    /// If this data was sent as a reliable or unreliable message.
    pub fn is_reliable(&self) -> bool {
        self.is_reliable
    }

    /// Access message data.
    pub fn data(&self) -> &Packet {
        &self.data
    }
}

/// Message-receiving part of the connection.
pub struct StreamReceiver {
    shared: Rc<Shared>,
    sequence: Sequence,
    reliable_queue: VecDeque<Option<QueueEntry>>,
    reliable_split_buffer: Vec<u8>,
    reliable_split_channel: Option<Channel>,
    unreliable_split_buffers: VecDeque<UnreliableBuffer>,
    transport_receiver: ChannelRx<InBuffer>,
    feedback: Feedback,
}

impl StreamReceiver {
    /// Receive a message.
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
                // First try to increment to prevent theoretical overflow:
                self.sequence = self
                    .sequence
                    .checked_add(1)
                    .ok_or(Error::InvalidConnection)?;

                let QueueEntry {
                    buffer: queue_buffer,
                    channel,
                    is_split,
                } = self.reliable_queue.pop_front().flatten().unwrap();

                self.reliable_queue.push_back(None);

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

                    if self.reliable_split_buffer.len() + queue_buffer.as_ref().len()
                        > MAX_SPLIT_DATA_SIZE
                    {
                        return Err(Error::PeerMessageTooLarge);
                    }

                    self.reliable_split_buffer
                        .extend_from_slice(queue_buffer.as_ref());

                    continue;
                } else {
                    let buf = if let Some(reliable_split_channel) = self.reliable_split_channel {
                        // Split just completed, extending and returning
                        if reliable_split_channel == channel {
                            if self.reliable_split_buffer.len() + queue_buffer.as_ref().len()
                                > MAX_SPLIT_DATA_SIZE
                            {
                                return Err(Error::PeerMessageTooLarge);
                            }

                            self.reliable_split_buffer
                                .extend_from_slice(queue_buffer.as_ref());

                            self.reliable_split_channel = None;

                            mem::take(&mut self.reliable_split_buffer).into()
                        } else {
                            continue;
                        }
                    } else {
                        // Non-split packet arrived
                        queue_buffer.into()
                    };

                    return Ok(ReceivedData {
                        channel,
                        is_reliable: true,
                        data: buf,
                    });
                }
            }

            let InBuffer {
                buffer: mut in_buffer,
                stop,
            } = {
                #[cfg(feature = "single")]
                {
                    self.transport_receiver.recv()
                }
                #[cfg(feature = "multi")]
                {
                    self.transport_receiver.recv_async()
                }
            }
            .await
            .map_err(|_| Error::ServerWasDropped)?;

            let packet_type = in_buffer.as_ref()[TYPE_INDEX];

            if crate::decode_in_buffer(&mut in_buffer.as_mut()[.. stop], &self.shared.cipher)
                .is_err()
            {
                continue;
            };

            let mut in_buffer = in_buffer.finish(ENCRYPTION_START, stop);

            let mut read_cursor = Cursor::new(in_buffer.as_ref());

            match packet_type {
                Type::DISCONNECT => {
                    return Err(Error::Disconnect);
                },
                Type::UNRELIABLE => {
                    let channel: Channel = seek_read!(read_cursor.read_bytes(), "channel");
                    in_buffer.start += read_cursor.position().to_usize();
                    return Ok(ReceivedData {
                        channel,
                        is_reliable: false,
                        data: in_buffer.into(),
                    });
                },
                Type::UNRELIABLE_SPLIT_START => {
                    let channel: Channel = seek_read!(read_cursor.read_bytes(), "channel");
                    let split_id: SplitId = seek_read!(read_cursor.read_bytes(), "split_id");
                    let expected_packets: u32 =
                        seek_read!(read_cursor.read_bytes(), "expected_packets");

                    if expected_packets > MAX_SPLIT_PACKETS {
                        log::debug!(
                            "dropping packet with packet length {} more than maximum {}",
                            expected_packets,
                            MAX_SPLIT_PACKETS,
                        );
                        continue;
                    }

                    let mut split_buffer = if self.unreliable_split_buffers.len()
                        == UNRELIABLE_BUFFERS
                        || self.unreliable_split_buffers.back().is_some()
                            && self.unreliable_split_buffers.back().unwrap().is_complete()
                    {
                        let mut b = self.unreliable_split_buffers.pop_back().unwrap();
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
                        b_shards.resize(expected_packets.to_usize(), UnreliableBufferShard::new());
                        b
                    } else {
                        UnreliableBuffer {
                            split_id,
                            channel,
                            complete_shards: 0,
                            shards: vec![UnreliableBufferShard::new(); expected_packets.to_usize()],
                        }
                    };

                    in_buffer.start += read_cursor.position().to_usize();
                    let in_buffer: &[u8] = in_buffer.as_ref();

                    let shard = split_buffer.shards.get_mut(0).unwrap();

                    shard.buffer[.. in_buffer.len()].copy_from_slice(in_buffer);
                    shard.length = in_buffer.len();
                    shard.written = true;

                    split_buffer.complete_shards += 1;
                    self.unreliable_split_buffers.push_front(split_buffer);
                },
                Type::UNRELIABLE_SPLIT => {
                    let channel: Channel = seek_read!(read_cursor.read_bytes(), "channel");
                    let split_id: SplitId = seek_read!(read_cursor.read_bytes(), "split_id");
                    let count: u32 = seek_read!(read_cursor.read_bytes(), "count");

                    in_buffer.start += read_cursor.position().to_usize();
                    let in_buffer: &[u8] = in_buffer.as_ref();

                    let split_buffer = match self.unreliable_split_buffers.iter_mut().find(|b| {
                        b.split_id == split_id && b.channel == channel && !b.is_complete()
                    }) {
                        Some(b) => b,
                        None => continue,
                    };

                    let shard = match split_buffer.shards.get_mut(count.to_usize()) {
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

                    let shard = split_buffer.shards.get_mut(count.to_usize()).unwrap();

                    shard.buffer[.. in_buffer.len()].copy_from_slice(in_buffer);
                    shard.length = in_buffer.len();
                    shard.written = true;

                    split_buffer.complete_shards += 1;

                    if split_buffer.is_complete() {
                        let mut buf = Vec::with_capacity(MAX_DATA_SIZE * split_buffer.shards.len());

                        for shard in split_buffer.shards.iter() {
                            buf.extend_from_slice(&shard.buffer[.. shard.length]);
                        }

                        // TODO: also check CRC and if it's incorrect restore buf length to
                        // MAX_PACKET_SIZE before continuing

                        return Ok(ReceivedData {
                            channel,
                            is_reliable: false,
                            data: buf.into(),
                        });
                    }
                },
                Type::RELIABLE | Type::RELIABLE_SPLIT => {
                    let channel: Channel = seek_read!(read_cursor.read_bytes(), "channel");
                    let sequence: Sequence = seek_read!(read_cursor.read_bytes(), "sequence");

                    if let Some(index) = sequence.checked_sub(self.sequence) {
                        in_buffer.start += read_cursor.position().to_usize();

                        if index < RELIABLE_QUEUE_LENGTH {
                            let queue_place =
                                self.reliable_queue.get_mut(index.to_usize()).unwrap();

                            *queue_place = Some(QueueEntry {
                                buffer: in_buffer,
                                channel,
                                is_split: packet_type == Type::RELIABLE_SPLIT,
                            });
                        }
                    }

                    if sequence.abs_diff(self.sequence) <= RELIABLE_QUEUE_LENGTH {
                        stream_send_ack(&self.shared, &mut self.feedback, sequence).await?;
                    }
                },
                _ => {},
            }
        }
    }
}

struct Client {
    address: SocketAddr,
    ack_sender: ChannelTx<InBuffer>,
    in_queue: ChannelTx<InBuffer>,
}

struct Clients {
    clients: Vec<Option<Client>>,
    free_indices: Vec<Id>,
}

impl Clients {
    // first two ids are reserved for server and a new connection
    const ID_OFFSET: Id = 2;

    fn new(max_clients: Id) -> Self {
        Self {
            clients: (0 .. max_clients).map(|_| None).collect(),
            free_indices: (0 .. max_clients).rev().collect(),
        }
    }

    fn get(&self, id: Id) -> Option<&Client> {
        if id < Self::ID_OFFSET {
            return None;
        }
        self.clients
            .get((id - Self::ID_OFFSET).to_usize())?
            .as_ref()
    }

    fn get_mut(&mut self, id: Id) -> Option<&mut Client> {
        if id < Self::ID_OFFSET {
            return None;
        }
        self.clients
            .get_mut((id - Self::ID_OFFSET).to_usize())?
            .as_mut()
    }

    fn push(&mut self, client: Client) -> Option<Id> {
        self.free_indices.pop().map(|idx| {
            *self.clients.get_mut(idx.to_usize()).unwrap() = Some(client);
            idx + Self::ID_OFFSET
        })
    }

    fn remove(&mut self, id: Id) -> Option<Client> {
        if id < Self::ID_OFFSET {
            return None;
        }

        let idx = id - Self::ID_OFFSET;
        let res = mem::replace(self.clients.get_mut(idx.to_usize())?, None);

        if res.is_some() {
            self.free_indices.push(idx);
        }

        res
    }
}

enum ServerPacket {
    In((usize, SocketAddr)),
    Out(Out),
}

/// Returned on successful `Server::accept()`.
/// Represents a fresh connection with a new client.
pub struct Connection {
    /// One-time own public key.
    /// In combination with some secret, can be used to verify server's own the identity for the client.
    pub self_key: Key,
    /// One-time client public key.
    /// In combination with some secret, can be used to verify the identity of the connected client.
    pub peer_key: Key,
    /// Sender part that has both reliable and unreliable functionality included.
    pub sender: StreamSender,
    /// Receiver part.
    pub receiver: StreamReceiver,
}

/// Server parameters.
#[derive(Debug)]
pub struct ServerParameters {
    /// Maximum number of simultaneous connections that the server can have.
    pub max_connections: Id,
}

impl Default for ServerParameters {
    fn default() -> Self {
        Self {
            max_connections: DEFAULT_MAX_CONNECTIONS,
        }
    }
}

impl ServerParameters {
    /// Bind the socket and produce a `Server` with the given parameters.
    pub async fn bind<A>(self, bind_address: A) -> Result<Server, StdIoError>
    where
        A: Into<SocketAddr>,
    {
        let transport = UdpSocket::bind(bind_address.into()).await?;
        let (out_queue_sender, out_queue) = new_channel();
        Ok(Server {
            clients: Clients::new(self.max_connections),
            out_queue,
            out_queue_sender,
            receive_buffer: WriteBuffer::new(),
            transport,
            send_first: true,
        })
    }
}

pub struct Server {
    clients: Clients,
    out_queue: ChannelRx<Out>,
    out_queue_sender: ChannelTx<Out>,
    receive_buffer: WriteBuffer,
    transport: UdpSocket,
    send_first: bool,
}

impl Server {
    /// Accept a new connection.
    ///
    /// **Internally, this method handles most of the message routing from and to connection
    /// streams. This means that the futures returned by the method must be polled in a loop
    /// constantly for the existing connections to work.**
    pub async fn accept(&mut self) -> Result<Connection, Error> {
        loop {
            let next = {
                let send_future = async {
                    // Server struct exists (because &mut self), the following will never panic,
                    // since we have one sender kept in the struct

                    #[cfg(feature = "single")]
                    let out_packet = self.out_queue.recv().await.unwrap();

                    #[cfg(feature = "multi")]
                    let out_packet = self.out_queue.recv_async().await.unwrap();

                    Ok::<_, StdIoError>(ServerPacket::Out(out_packet))
                };

                let mut send_future = pin!(send_future);

                let recv_future = async {
                    Ok::<_, StdIoError>(ServerPacket::In(
                        self.transport
                            .recv_from(self.receive_buffer.as_mut())
                            .await?,
                    ))
                };

                let mut recv_future = pin!(recv_future);

                let combined = std::future::poll_fn(|ctx| {
                    if self.send_first {
                        self.send_first = false;
                        match send_future.as_mut().poll(ctx) {
                            o @ Poll::Ready(_) => return o,
                            Poll::Pending => recv_future.as_mut().poll(ctx),
                        }
                    } else {
                        self.send_first = true;
                        match recv_future.as_mut().poll(ctx) {
                            o @ Poll::Ready(_) => return o,
                            Poll::Pending => send_future.as_mut().poll(ctx),
                        }
                    }
                });

                combined.await
            }?;

            match next {
                ServerPacket::In((len, addr)) => {
                    let mut read_cursor = Cursor::new(self.receive_buffer.as_ref());
                    let sender: Id = seek_read!(read_cursor.read_bytes(), "sender");

                    let mut packet_type = Type::UNDEFINED;
                    seek_read!(
                        read_cursor.read_exact(slice::from_mut(&mut packet_type)),
                        "type"
                    );

                    match packet_type {
                        Type::CONNECT => {
                            if sender != NEW_CONNECTION_ID {
                                debug!("received non-connection message");
                                continue;
                            }

                            let mut peer_key = KEY_BUFFER;
                            seek_read!(read_cursor.read_exact(&mut peer_key), "peer key");
                            let deciphered_peer_key = seek_read!(
                                PublicKey::from_sec1_bytes(&peer_key),
                                "deciphered peer key"
                            );

                            let keypair = EphemeralSecret::random(&mut OsRng);
                            let mut secret = SECRET_BUFFER;
                            keypair
                                .diffie_hellman(&deciphered_peer_key)
                                .extract::<sha2::Sha256>(None)
                                .expand(&[], &mut secret)
                                .unwrap();

                            let cipher = ChaCha20Poly1305::new((&secret).into());

                            let (in_queue_tx, in_queue_rx) = new_channel();

                            let (ack_sender, ack_receiver) = new_channel();

                            let client = Client {
                                address: addr,
                                ack_sender,
                                in_queue: in_queue_tx,
                            };

                            let id = match self.clients.push(client) {
                                Some(id) => id,
                                None => {
                                    // TODO send disconnect/decline?
                                    continue;
                                },
                            };

                            let self_key: Key = EncodedPoint::from(keypair.public_key())
                                .as_bytes()
                                .try_into()
                                .unwrap();

                            let mut write_cursor = Cursor::new(self.receive_buffer.as_mut_slice());

                            write_cursor.write_bytes(SERVER_ID).unwrap();
                            write_cursor.write_bytes(Type::ACCEPT).unwrap();
                            write_cursor.write_all(&self_key).unwrap();
                            write_cursor.write_bytes(id).unwrap();

                            if self
                                .transport
                                .send_to(write_cursor.slice(), addr)
                                .await
                                .is_err()
                            {
                                self.clients.remove(id);
                                continue;
                            };

                            let shared = Shared {
                                peer: id,
                                cipher,
                                transport_sender: self.out_queue_sender.clone(),
                            };

                            let shared = Rc::new(shared);

                            return Ok(Connection {
                                self_key,
                                peer_key,
                                sender: StreamSender {
                                    unreliable: StreamUnreliableSender {
                                        shared: shared.clone(),
                                        unreliable_split_id: 0,
                                        feedback: Feedback::new(),
                                    },
                                    reliable: StreamReliableSender {
                                        shared: shared.clone(),
                                        queue_front_sequence: 0,
                                        queue: VecDeque::new(),
                                        ack_receiver,
                                        feedback: Feedback::new(),
                                    },
                                },
                                receiver: StreamReceiver {
                                    shared,
                                    sequence: 0,
                                    reliable_queue: vec![None; RELIABLE_QUEUE_LENGTH as usize]
                                        .into(),
                                    reliable_split_buffer: Vec::new(),
                                    reliable_split_channel: None,
                                    unreliable_split_buffers: VecDeque::with_capacity(
                                        UNRELIABLE_BUFFERS,
                                    ),
                                    transport_receiver: in_queue_rx,
                                    feedback: Feedback::new(),
                                },
                            });
                        },
                        Type::ACKNOWLEDGE => {
                            if let Some(client) = self.clients.get_mut(sender) {
                                let data = InBuffer {
                                    buffer: mem::replace(
                                        &mut self.receive_buffer,
                                        WriteBuffer::new(),
                                    ),
                                    stop: len,
                                };

                                let _ = client.ack_sender.send(data);
                            }
                        },
                        _ => {
                            if let Some(client) = self.clients.get_mut(sender) {
                                let data = InBuffer {
                                    buffer: mem::replace(
                                        &mut self.receive_buffer,
                                        WriteBuffer::new(),
                                    ),
                                    stop: len,
                                };

                                let _ = client.in_queue.send(data);
                            }
                        },
                    }
                },
                ServerPacket::Out(out) => {
                    match out {
                        Out::Buffer {
                            peer,
                            buffer,
                            result_tx,
                        } => {
                            let client = match self.clients.get(peer) {
                                Some(c) => c,
                                None => {
                                    let _ = result_tx.send(Err(Error::InvalidConnection));
                                    continue;
                                },
                            };

                            if let Err(err) = self
                                .transport
                                .send_to(buffer.as_ref(), client.address)
                                .await
                            {
                                let _ = result_tx.send(Err(err.into()));
                            } else {
                                let _ = result_tx.send(Ok(()));
                            }
                        },
                        Out::DropClient { peer, cipher } => {
                            if let Some(client) = self.clients.remove(peer) {
                                let mut write_cursor =
                                    Cursor::new(self.receive_buffer.as_mut_slice());

                                write_cursor.write_bytes(SERVER_ID).unwrap();
                                write_cursor.write_bytes(Type::DISCONNECT).unwrap();

                                crate::tag_sign_in_buffer(self.receive_buffer.as_mut(), &cipher);

                                let _ = self
                                    .transport
                                    .send_to(
                                        &self.receive_buffer.as_ref()[.. ENCRYPTION_START],
                                        client.address,
                                    )
                                    .await;
                            }
                        },
                    }
                },
            }
        }
    }
}
