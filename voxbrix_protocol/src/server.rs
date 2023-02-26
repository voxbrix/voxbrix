use crate::{
    seek_read,
    AsSlice,
    Channel,
    Id,
    Key,
    Sequence,
    Type,
    UnreliableBuffer,
    KEY_BUFFER,
    MAX_DATA_SIZE,
    MAX_PACKET_SIZE,
    NEW_CONNECTION_ID,
    SECRET_BUFFER,
    SERVER_ID,
    UNRELIABLE_BUFFERS,
};
use async_io::{
    Async,
    Timer,
};
#[cfg(feature = "multi")]
use async_oneshot::{
    oneshot as new_oneshot,
    Sender as OneshotTx,
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
};
use futures_lite::future::FutureExt;
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
    oneshot::{
        oneshot as new_oneshot,
        Sender as OneshotTx,
    },
};
use log::warn;
use rand_core::OsRng;
#[cfg(feature = "single")]
use std::rc::Rc;
#[cfg(feature = "multi")]
use std::sync::Arc;
use std::{
    alloc::{
        self,
        Layout,
    },
    collections::{
        BTreeMap,
        BTreeSet,
        VecDeque,
    },
    fmt,
    io::{
        Cursor,
        Error as StdIoError,
        Read,
        Write,
    },
    iter,
    mem,
    net::{
        SocketAddr,
        UdpSocket,
    },
    slice,
    time::Duration,
};

pub const DEFAULT_MAX_CONNECTIONS: usize = 64;

#[derive(Debug)]
pub enum Error {
    Io(StdIoError),
    ServerWasDropped,
    Disconnect,
    InvalidConnection,
    ServerIsFull,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {}

impl From<StdIoError> for Error {
    fn from(from: StdIoError) -> Self {
        Self::Io(from)
    }
}

struct Buffer {
    // Box to avoid bloating enums that use Packet
    buffer: Box<[u8; MAX_PACKET_SIZE]>,
    start: usize,
    stop: usize,
}

impl Buffer {
    fn allocate() -> Box<[u8; MAX_PACKET_SIZE]> {
        // SAFETY: fast and safe way to get Box of [0u8; MAX_PACKET_SIZE]
        // without copying stack to heap (as would be with Box::new())
        // https://doc.rust-lang.org/std/boxed/index.html#memory-layout
        unsafe {
            let layout = Layout::new::<[u8; MAX_PACKET_SIZE]>();
            let ptr = alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                alloc::handle_alloc_error(layout);
            }
            Box::from_raw(ptr.cast())
        }
    }
}

impl AsRef<[u8]> for Buffer {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[self.start .. self.stop]
    }
}

impl AsMut<[u8]> for Buffer {
    fn as_mut(&mut self) -> &mut [u8] {
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

struct InBuffer {
    packet_type: u8,
    buffer: Buffer,
}

enum Out {
    Buffer {
        peer: Id,
        buffer: Buffer,
        tag_start: usize,
        result_tx: OneshotTx<Result<(), Error>>,
    },
    DropClient {
        peer: Id,
    },
}

async fn stream_send_ack(shared: &Shared, sequence: Sequence) -> Result<(), Error> {
    let mut buffer = Buffer::allocate();

    let (tag_start, stop) =
        crate::write_in_buffer(&mut buffer, SERVER_ID, Type::ACKNOWLEDGE, |cursor| {
            cursor.write_varint(sequence).unwrap();
        });

    let (result_tx, result_rx) = new_oneshot();

    shared
        .transport_sender
        .send(Out::Buffer {
            peer: shared.peer,
            buffer: Buffer {
                buffer,
                start: 0,
                stop,
            },
            tag_start,
            result_tx,
        })
        .map_err(|_| Error::ServerWasDropped)?;

    #[cfg(feature = "single")]
    result_rx.await.ok_or(Error::ServerWasDropped)??;
    #[cfg(feature = "multi")]
    result_rx.await.map_err(|_| Error::ServerWasDropped)??;

    Ok(())
}

struct Shared {
    peer: Id,
    transport_sender: ChannelTx<Out>,
}

impl Drop for Shared {
    fn drop(&mut self) {
        let _ = self
            .transport_sender
            .send(Out::DropClient { peer: self.peer });
    }
}

pub struct StreamSender {
    unreliable: StreamUnreliableSender,
    reliable: StreamReliableSender,
}

impl StreamSender {
    pub async fn send_unreliable(&mut self, channel: usize, data: &[u8]) -> Result<(), Error> {
        self.unreliable.send_unreliable(channel, data).await
    }

    pub async fn send_reliable(&mut self, channel: usize, data: &[u8]) -> Result<(), Error> {
        self.reliable.send_reliable(channel, data).await
    }

    pub fn split(self) -> (StreamUnreliableSender, StreamReliableSender) {
        let Self {
            unreliable,
            reliable,
        } = self;

        (unreliable, reliable)
    }
}

pub struct StreamUnreliableSender {
    #[cfg(feature = "single")]
    shared: Rc<Shared>,
    #[cfg(feature = "multi")]
    shared: Arc<Shared>,
    unreliable_split_id: u16,
}

impl StreamUnreliableSender {
    async fn send_unreliable_one(
        &self,
        channel: usize,
        data: &[u8],
        packet_type: u8,
        len_or_count: Option<usize>,
    ) -> Result<(), Error> {
        let mut buffer = Buffer::allocate();

        let (tag_start, stop) =
            crate::write_in_buffer(&mut buffer, SERVER_ID, packet_type, |cursor| {
                cursor.write_varint(channel).unwrap();
                if let Some(len_or_count) = len_or_count {
                    cursor.write_varint(self.unreliable_split_id).unwrap();
                    cursor.write_varint(len_or_count).unwrap();
                }
                cursor.write_all(data).unwrap();
            });

        let (result_tx, result_rx) = new_oneshot();

        self.shared
            .transport_sender
            .send(Out::Buffer {
                peer: self.shared.peer,
                buffer: Buffer {
                    buffer,
                    start: 0,
                    stop,
                },
                tag_start,
                result_tx,
            })
            .map_err(|_| Error::ServerWasDropped)?;

        #[cfg(feature = "single")]
        result_rx.await.ok_or(Error::ServerWasDropped)??;
        #[cfg(feature = "multi")]
        result_rx.await.map_err(|_| Error::ServerWasDropped)??;

        Ok(())
    }

    pub async fn send_unreliable(&mut self, channel: usize, data: &[u8]) -> Result<(), Error> {
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
                let end = start + (data.len() - start).min(MAX_DATA_SIZE);
                self.send_unreliable_one(
                    channel,
                    &data[start .. end],
                    Type::UNRELIABLE_SPLIT,
                    Some(count),
                )
                .await?;
                start += MAX_DATA_SIZE;
            }

            Ok(())
        } else {
            self.send_unreliable_one(channel, data, Type::UNRELIABLE, None)
                .await
        }
    }
}

pub struct StreamReliableSender {
    #[cfg(feature = "single")]
    shared: Rc<Shared>,
    #[cfg(feature = "multi")]
    shared: Arc<Shared>,
    sequence: Sequence,
    ack_receiver: ChannelRx<Sequence>,
}

impl StreamReliableSender {
    async fn send_reliable_one(
        &mut self,
        channel: usize,
        data: &[u8],
        packet_type: u8,
    ) -> Result<(), Error> {
        loop {
            let mut buffer = Buffer::allocate();

            let (tag_start, stop) =
                crate::write_in_buffer(&mut buffer, SERVER_ID, packet_type, |cursor| {
                    cursor.write_varint(channel).unwrap();
                    cursor.write_varint(self.sequence).unwrap();
                    cursor.write_all(data).unwrap();
                });

            let (result_tx, result_rx) = new_oneshot();

            self.shared
                .transport_sender
                .send(Out::Buffer {
                    peer: self.shared.peer,
                    buffer: Buffer {
                        buffer,
                        start: 0,
                        stop,
                    },
                    tag_start,
                    result_tx,
                })
                .map_err(|_| Error::ServerWasDropped)?;

            #[cfg(feature = "single")]
            result_rx.await.ok_or(Error::ServerWasDropped)??;
            #[cfg(feature = "multi")]
            result_rx.await.map_err(|_| Error::ServerWasDropped)??;

            let result = async {
                #[cfg(feature = "single")]
                while let Some(ack) = self.ack_receiver.recv().await {
                    if ack == self.sequence {
                        return Ok(());
                    }
                }

                #[cfg(feature = "multi")]
                while let Ok(ack) = self.ack_receiver.recv_async().await {
                    if ack == self.sequence {
                        return Ok(());
                    }
                }

                Err(Some(Error::ServerWasDropped))
            }
            .or(async {
                Timer::after(Duration::from_secs(1)).await;
                Err(None)
            })
            .await;

            match result {
                Ok(()) => {
                    self.sequence = self.sequence.wrapping_add(1);
                    return Ok(());
                },
                Err(Some(err)) => return Err(err),
                _ => {},
            }
        }
    }

    pub async fn send_reliable(&mut self, channel: usize, data: &[u8]) -> Result<(), Error> {
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
}

pub struct StreamReceiver {
    #[cfg(feature = "single")]
    shared: Rc<Shared>,
    #[cfg(feature = "multi")]
    shared: Arc<Shared>,
    sequence: Sequence,
    split_buffer: Vec<u8>,
    split_channel: Option<Channel>,
    unreliable_split_buffers: VecDeque<UnreliableBuffer>,
    transport_receiver: ChannelRx<InBuffer>,
}

impl StreamReceiver {
    pub async fn recv<'a>(&mut self) -> Result<(Channel, Packet), Error> {
        loop {
            #[cfg(feature = "single")]
            let InBuffer {
                packet_type,
                buffer: mut packet,
            } = self
                .transport_receiver
                .recv()
                .await
                .ok_or(Error::ServerWasDropped)?;

            #[cfg(feature = "multi")]
            let InBuffer {
                packet_type,
                buffer: mut packet,
            } = self
                .transport_receiver
                .recv_async()
                .await
                .map_err(|_| Error::ServerWasDropped)?;

            let mut read_cursor = Cursor::new(packet.as_ref());

            match packet_type {
                Type::DISCONNECT => {
                    return Err(Error::Disconnect);
                },
                Type::UNRELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    packet.start += read_cursor.position() as usize;
                    return Ok((channel, packet.into()));
                },
                Type::UNRELIABLE_SPLIT_START => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let split_id: u16 = seek_read!(read_cursor.read_varint(), "split_id");
                    let expected_length: usize = seek_read!(read_cursor.read_varint(), "length");

                    let mut split_buffer = if self.unreliable_split_buffers.len()
                        == UNRELIABLE_BUFFERS
                        || self.unreliable_split_buffers.back().is_some()
                            && self.unreliable_split_buffers.back().unwrap().complete
                    {
                        let mut b = self.unreliable_split_buffers.pop_back().unwrap();
                        b.split_id = split_id;
                        b.channel = channel;
                        b.expected_length = expected_length;
                        b.existing_pieces.clear();
                        b.complete = false;
                        b
                    } else {
                        UnreliableBuffer {
                            split_id,
                            channel,
                            expected_length,
                            existing_pieces: BTreeSet::new(),
                            buffer: BTreeMap::new(),
                            complete: false,
                        }
                    };

                    packet.start += read_cursor.position() as usize;
                    let packet: &[u8] = packet.as_ref();

                    match split_buffer.buffer.get_mut(&0) {
                        Some((current_length, shard)) => {
                            shard[.. packet.len()].copy_from_slice(packet);
                            *current_length = packet.len();
                        },
                        None => {
                            let mut new_shard = [0u8; MAX_DATA_SIZE];
                            new_shard[.. packet.len()].copy_from_slice(packet);
                            split_buffer.buffer.insert(0, (packet.len(), new_shard));
                        },
                    }

                    split_buffer.existing_pieces.insert(0);
                    self.unreliable_split_buffers.push_front(split_buffer);
                },
                Type::UNRELIABLE_SPLIT => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let split_id: u16 = seek_read!(read_cursor.read_varint(), "split_id");
                    let count: usize = seek_read!(read_cursor.read_varint(), "count");

                    packet.start += read_cursor.position() as usize;
                    let packet: &[u8] = packet.as_ref();

                    let split_buffer = match self
                        .unreliable_split_buffers
                        .iter_mut()
                        .find(|b| b.split_id == split_id && b.channel == channel && !b.complete)
                    {
                        Some(b) => b,
                        None => continue,
                    };

                    match split_buffer.buffer.get_mut(&count) {
                        Some((current_length, shard)) => {
                            shard[.. packet.len()].copy_from_slice(packet);
                            *current_length = packet.len();
                        },
                        None => {
                            let mut new_shard = [0u8; MAX_DATA_SIZE];
                            new_shard[.. packet.len()].copy_from_slice(packet);
                            split_buffer.buffer.insert(count, (packet.len(), new_shard));
                        },
                    }

                    split_buffer.existing_pieces.insert(count);

                    if split_buffer
                        .existing_pieces
                        .range(0 .. split_buffer.expected_length)
                        .count()
                        == split_buffer.expected_length
                    {
                        let mut buf =
                            Vec::with_capacity(MAX_DATA_SIZE * split_buffer.expected_length);

                        for (_, (len, data)) in
                            split_buffer.buffer.range(0 .. split_buffer.expected_length)
                        {
                            buf.extend_from_slice(&data[.. *len]);
                        }

                        // TODO: also check CRC and if it's incorrect restore buf length to
                        // MAX_PACKET_SIZE before continuing

                        split_buffer.complete = true;

                        return Ok((channel, buf.into()));
                    }
                },
                Type::RELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    // TODO: do not answer if the sequence is not previous, but random?
                    stream_send_ack(&self.shared, sequence).await?;

                    if sequence == self.sequence {
                        self.sequence = self.sequence.wrapping_add(1);

                        if self.split_channel.is_none() {
                            packet.start += read_cursor.position() as usize;
                            return Ok((channel, packet.into()));
                        } else {
                            let offset = read_cursor.position() as usize;

                            self.split_buffer
                                .extend_from_slice(&packet.as_ref()[offset ..]);

                            self.split_channel = None;

                            return Ok((channel, mem::take(&mut self.split_buffer).into()));
                        }
                    }
                },
                Type::RELIABLE_SPLIT => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");

                    if self.split_channel.is_none() {
                        self.split_channel = Some(channel);
                    } else if let Some(split_channel) = self.split_channel {
                        if split_channel != channel {
                            warn!("skipping mishappened packet with channel {}", channel);
                            continue;
                        }
                    }

                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    // TODO: do not answer if the sequence is not previous, but random?
                    stream_send_ack(&self.shared, sequence).await?;

                    if sequence == self.sequence {
                        self.sequence = self.sequence.wrapping_add(1);
                        let offset = read_cursor.position() as usize;

                        self.split_buffer
                            .extend_from_slice(&packet.as_ref()[offset ..]);
                    }
                },
                _ => {},
            }
        }
    }
}

struct Client {
    cipher: ChaCha20Poly1305,
    address: SocketAddr,
    ack_sender: ChannelTx<Sequence>,
    in_queue: ChannelTx<InBuffer>,
}

struct Clients {
    max_clients: usize,
    curr_clients: usize,
    clients: Vec<Option<Client>>,
    free_indices: VecDeque<Id>,
}

impl Clients {
    // first two ids are reserved for server and a new connection
    const ID_OFFSET: usize = 2;

    fn new(max_clients: usize) -> Self {
        Self {
            max_clients,
            curr_clients: 0,
            clients: Vec::with_capacity(max_clients),
            free_indices: VecDeque::new(),
        }
    }

    fn get(&self, id: Id) -> Option<&Client> {
        if id < Self::ID_OFFSET {
            return None;
        }
        self.clients.get(id - Self::ID_OFFSET)?.as_ref()
    }

    fn get_mut(&mut self, id: Id) -> Option<&mut Client> {
        if id < Self::ID_OFFSET {
            return None;
        }
        self.clients.get_mut(id - Self::ID_OFFSET)?.as_mut()
    }

    fn push(&mut self, client: Client) -> Option<usize> {
        match iter::from_fn(|| self.free_indices.pop_front())
            .find(|idx| self.clients.get(*idx).is_some())
        {
            Some(idx) => {
                *self.clients.get_mut(idx).unwrap() = Some(client);
                self.curr_clients += 1;
                Some(idx + Self::ID_OFFSET)
            },
            None => {
                if self.curr_clients < self.max_clients {
                    self.clients.push(Some(client));
                    self.curr_clients += 1;
                    Some(self.clients.len() + Self::ID_OFFSET - 1)
                } else {
                    None
                }
            },
        }
    }

    fn remove(&mut self, id: Id) -> Option<Client> {
        if id < Self::ID_OFFSET {
            return None;
        }

        let idx = id - Self::ID_OFFSET;
        let res = mem::replace(self.clients.get_mut(idx)?, None);

        if res.is_some() {
            self.curr_clients -= 1;
            self.free_indices.push_back(idx);

            while let Some(None) = self.clients.last() {
                self.clients.pop();
            }
        }

        res
    }
}

enum ServerPacket {
    In((usize, SocketAddr)),
    Out(Out),
}

pub struct Connection {
    pub self_key: Key,
    pub peer_key: Key,
    pub sender: StreamSender,
    pub receiver: StreamReceiver,
}

#[derive(Debug)]
pub struct ServerParameters {
    pub max_connections: usize,
}

impl Default for ServerParameters {
    fn default() -> Self {
        Self {
            max_connections: DEFAULT_MAX_CONNECTIONS,
        }
    }
}

impl ServerParameters {
    pub fn bind<A>(self, bind_address: A) -> Result<Server, StdIoError>
    where
        A: Into<SocketAddr>,
    {
        let transport = Async::<UdpSocket>::bind(bind_address.into())?;
        let (out_queue_sender, out_queue) = new_channel();
        Ok(Server {
            clients: Clients::new(self.max_connections),
            out_queue,
            out_queue_sender,
            receive_buffer: Buffer::allocate(),
            transport,
        })
    }
}

pub struct Server {
    clients: Clients,
    out_queue: ChannelRx<Out>,
    out_queue_sender: ChannelTx<Out>,
    receive_buffer: Box<[u8; MAX_PACKET_SIZE]>,
    transport: Async<UdpSocket>,
}

impl Server {
    pub async fn accept(&mut self) -> Result<Connection, StdIoError> {
        loop {
            let next: Result<_, StdIoError> = async {
                // Server struct exists (because &mut self), the following will never panic,
                // since we have one sender kept in the struct

                #[cfg(feature = "single")]
                let out_packet = self.out_queue.recv().await.unwrap();

                #[cfg(feature = "multi")]
                let out_packet = self.out_queue.recv_async().await.unwrap();

                Ok(ServerPacket::Out(out_packet))
            }
            .or(async {
                Ok(ServerPacket::In(
                    self.transport
                        .recv_from(self.receive_buffer.as_mut_slice())
                        .await?,
                ))
            })
            .await;

            match next? {
                ServerPacket::In((len, addr)) => {
                    let mut read_cursor = Cursor::new(self.receive_buffer.as_slice());
                    let sender: usize = seek_read!(read_cursor.read_varint(), "sender");

                    let mut packet_type = Type::UNDEFINED;
                    seek_read!(
                        read_cursor.read_exact(slice::from_mut(&mut packet_type)),
                        "type"
                    );

                    match packet_type {
                        Type::CONNECT => {
                            if sender != NEW_CONNECTION_ID {
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
                                cipher,
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

                            write_cursor.write_varint(SERVER_ID).unwrap();
                            write_cursor.write_varint(Type::ACCEPT).unwrap();
                            write_cursor.write_all(&self_key).unwrap();
                            write_cursor.write_varint(id).unwrap();

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
                                transport_sender: self.out_queue_sender.clone(),
                            };

                            #[cfg(feature = "single")]
                            let shared = Rc::new(shared);

                            #[cfg(feature = "multi")]
                            let shared = Arc::new(shared);

                            return Ok(Connection {
                                self_key,
                                peer_key,
                                sender: StreamSender {
                                    unreliable: StreamUnreliableSender {
                                        shared: shared.clone(),
                                        unreliable_split_id: 0,
                                    },
                                    reliable: StreamReliableSender {
                                        shared: shared.clone(),
                                        sequence: 0,
                                        ack_receiver,
                                    },
                                },
                                receiver: StreamReceiver {
                                    shared: shared.clone(),
                                    sequence: 0,
                                    split_buffer: Vec::new(),
                                    split_channel: None,
                                    unreliable_split_buffers: VecDeque::with_capacity(
                                        UNRELIABLE_BUFFERS,
                                    ),
                                    transport_receiver: in_queue_rx,
                                },
                            });
                        },
                        Type::ACKNOWLEDGE => {
                            if let Some(client) = self.clients.get_mut(sender) {
                                let tag_start = read_cursor.position() as usize;
                                let decrypted_start = match crate::decode_in_buffer(
                                    &mut self.receive_buffer[.. len],
                                    tag_start,
                                    &client.cipher,
                                ) {
                                    Ok(s) => s,
                                    Err(()) => continue,
                                };

                                let mut read_cursor =
                                    Cursor::new(&self.receive_buffer[decrypted_start .. len]);
                                let sequence = seek_read!(read_cursor.read_varint(), "sequence");

                                let _ = client.ack_sender.send(sequence);
                            }
                        },
                        _ => {
                            if let Some(client) = self.clients.get_mut(sender) {
                                let tag_start = read_cursor.position() as usize;
                                let start = match crate::decode_in_buffer(
                                    &mut self.receive_buffer[.. len],
                                    tag_start,
                                    &client.cipher,
                                ) {
                                    Ok(s) => s,
                                    Err(()) => continue,
                                };

                                let data = InBuffer {
                                    packet_type,
                                    buffer: Buffer {
                                        buffer: mem::replace(
                                            &mut self.receive_buffer,
                                            Buffer::allocate(),
                                        ),
                                        start,
                                        stop: len,
                                    },
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
                            mut buffer,
                            tag_start,
                            result_tx,
                        } => {
                            #[cfg(feature = "multi")]
                            let mut result_tx = result_tx;

                            let client = match self.clients.get(peer) {
                                Some(c) => c,
                                None => {
                                    let _ = result_tx.send(Err(Error::InvalidConnection));
                                    continue;
                                },
                            };

                            crate::encode_in_buffer(
                                &mut buffer.buffer,
                                &client.cipher,
                                tag_start,
                                buffer.stop,
                            );

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
                        Out::DropClient { peer } => {
                            self.clients.remove(peer);
                        },
                    }
                },
            }
        }
    }
}
