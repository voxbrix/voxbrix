use super::{
    seek_read,
    seek_write,
    AsSlice,
    Buffer,
    Channel,
    Id,
    Packet,
    Sequence,
    Type,
    UnreliableBuffer,
    ACKNOWLEDGE_SIZE,
    MAX_DATA_SIZE,
    MAX_PACKET_SIZE,
    SERVER_ID,
};
use crate::{
    Key,
    KEY_BUFFER,
    SECRET_BUFFER,
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
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
        HashMap,
        VecDeque,
    },
    fmt,
    io::{
        Cursor,
        Error as StdIoError,
        Read,
        Write,
    },
    mem,
    net::{
        SocketAddr,
        UdpSocket,
    },
    slice,
    time::Duration,
};

#[derive(Debug)]
pub enum Error {
    Io(StdIoError),
    ServerWasDropped,
    Disconnect,
    InvalidConnection,
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

struct TypedBuffer {
    sender: Id,
    packet_type: u8,
    packet: Buffer,
}

async fn stream_send_ack(
    cipher: &ChaCha20Poly1305,
    peer: Id,
    sequence: Sequence,
    transport: &mut ChannelTx<(Id, Buffer, OneshotTx<Result<(), Error>>)>,
) -> Result<(), Error> {
    let mut buffer = [0; MAX_PACKET_SIZE];

    let stop = crate::encode_in_buffer(
        &mut buffer,
        cipher,
        SERVER_ID,
        Type::ACKNOWLEDGE,
        |cursor| {
            cursor.write_varint(sequence).unwrap();
        },
    );

    let (result_tx, result_rx) = new_oneshot();

    transport
        .send((
            peer,
            Buffer {
                buffer,
                start: 0,
                stop,
            },
            result_tx,
        ))
        .map_err(|_| Error::ServerWasDropped)?;

    #[cfg(feature = "single")]
    result_rx.await.ok_or_else(|| Error::ServerWasDropped)??;
    #[cfg(feature = "multi")]
    result_rx.await.map_err(|_| Error::ServerWasDropped)??;

    Ok(())
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
    peer: Id,
    cipher: ChaCha20Poly1305,
    unreliable_split_id: u16,
    transport: ChannelTx<(Id, Buffer, OneshotTx<Result<(), Error>>)>,
}

impl StreamUnreliableSender {
    async fn send_unreliable_one(
        &self,
        channel: usize,
        data: &[u8],
        packet_type: u8,
        len_or_count: Option<usize>,
    ) -> Result<(), Error> {
        let mut buffer = [0; MAX_PACKET_SIZE];

        let stop = crate::encode_in_buffer(
            &mut buffer,
            &self.cipher,
            SERVER_ID,
            packet_type,
            |cursor| {
                cursor.write_varint(channel).unwrap();
                if let Some(len_or_count) = len_or_count {
                    cursor.write_varint(self.unreliable_split_id).unwrap();
                    cursor.write_varint(len_or_count).unwrap();
                }
                cursor.write_all(&data).unwrap();
            },
        );

        let (result_tx, result_rx) = new_oneshot();

        self.transport
            .send((
                self.peer,
                Buffer {
                    buffer,
                    start: 0,
                    stop,
                },
                result_tx,
            ))
            .map_err(|_| Error::ServerWasDropped)?;

        #[cfg(feature = "single")]
        result_rx.await.ok_or_else(|| Error::ServerWasDropped)??;
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
    peer: Id,
    cipher: ChaCha20Poly1305,
    sequence: Sequence,
    ack_receiver: ChannelRx<AckBuf>,
    transport: ChannelTx<(Id, Buffer, OneshotTx<Result<(), Error>>)>,
}

impl StreamReliableSender {
    async fn send_reliable_one(
        &mut self,
        channel: usize,
        data: &[u8],
        packet_type: u8,
    ) -> Result<(), Error> {
        let mut buffer = [0; MAX_PACKET_SIZE];

        let stop = crate::encode_in_buffer(
            &mut buffer,
            &self.cipher,
            SERVER_ID,
            packet_type,
            |cursor| {
                cursor.write_varint(channel).unwrap();
                cursor.write_varint(self.sequence).unwrap();
                cursor.write_all(data).unwrap();
            },
        );

        loop {
            let (result_tx, result_rx) = new_oneshot();

            self.transport
                .send((
                    self.peer,
                    Buffer {
                        buffer,
                        start: 0,
                        stop,
                    },
                    result_tx,
                ))
                .map_err(|_| Error::ServerWasDropped)?;

            #[cfg(feature = "single")]
            result_rx.await.ok_or_else(|| Error::ServerWasDropped)??;
            #[cfg(feature = "multi")]
            result_rx.await.map_err(|_| Error::ServerWasDropped)??;

            let result = async {
                #[cfg(feature = "single")]
                while let Some(AckBuf {
                    mut buffer,
                    tag_start,
                    length,
                }) = self.ack_receiver.recv().await
                {
                    let buffer = &mut buffer[.. length];
                    if crate::decode_in_buffer(buffer, tag_start, &self.cipher)
                        .and_then(|data_start| {
                            let sequence: u16 =
                                (&buffer[data_start ..]).read_varint().map_err(|_| ())?;
                            if sequence == self.sequence {
                                Ok(())
                            } else {
                                Err(())
                            }
                        })
                        .is_ok()
                    {
                        return Ok(());
                    }
                }

                #[cfg(feature = "multi")]
                while let Ok(AckBuf {
                    mut buffer,
                    tag_start,
                    length,
                }) = self.ack_receiver.recv_async().await
                {
                    let buffer = &mut buffer[.. length];
                    if crate::decode_in_buffer(buffer, tag_start, &self.cipher)
                        .and_then(|data_start| {
                            let sequence: u16 =
                                (&buffer[data_start ..]).read_varint().map_err(|_| ())?;
                            if sequence == self.sequence {
                                Ok(())
                            } else {
                                Err(())
                            }
                        })
                        .is_ok()
                    {
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
    cipher: ChaCha20Poly1305,
    sequence: Sequence,
    split_buffer: Vec<u8>,
    split_channel: Option<Channel>,
    unreliable_split_buffers: HashMap<Channel, UnreliableBuffer>,
    transport_sender: ChannelTx<(Id, Buffer, OneshotTx<Result<(), Error>>)>,
    transport_receiver: ChannelRx<TypedBuffer>,
}

impl StreamReceiver {
    pub async fn recv<'a>(&mut self) -> Result<(Channel, Packet), Error> {
        loop {
            #[cfg(feature = "single")]
            let TypedBuffer {
                sender,
                packet_type,
                mut packet,
            } = self
                .transport_receiver
                .recv()
                .await
                .ok_or_else(|| Error::ServerWasDropped)?;

            #[cfg(feature = "multi")]
            let TypedBuffer {
                sender,
                packet_type,
                mut packet,
            } = self
                .transport_receiver
                .recv_async()
                .await
                .map_err(|_| Error::ServerWasDropped)?;

            let data_start = match crate::decode_in_buffer(
                &mut packet.buffer[.. packet.stop],
                packet.start,
                &self.cipher,
            ) {
                Ok(s) => s,
                Err(_) => continue,
            };

            packet.start = data_start;

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

                    let split_buffer = match self.unreliable_split_buffers.get_mut(&channel) {
                        Some(b) => {
                            b.split_id = split_id;
                            b.expected_length = expected_length;
                            b.existing_pieces.clear();
                            b
                        },
                        None => {
                            self.unreliable_split_buffers.insert(
                                channel,
                                UnreliableBuffer {
                                    split_id,
                                    expected_length,
                                    existing_pieces: BTreeSet::new(),
                                    buffer: BTreeMap::new(),
                                },
                            );

                            self.unreliable_split_buffers.get_mut(&channel).unwrap()
                        },
                    };

                    packet.start += read_cursor.position() as usize;
                    let packet: &[u8] = packet.as_ref();

                    match split_buffer.buffer.get_mut(&0) {
                        Some((current_length, shard)) => {
                            (&mut shard[.. packet.len()]).copy_from_slice(&packet);
                            *current_length = packet.len();
                        },
                        None => {
                            let mut new_shard = [0u8; MAX_DATA_SIZE];
                            (&mut new_shard[.. packet.len()]).copy_from_slice(&packet);
                            split_buffer.buffer.insert(0, (packet.len(), new_shard));
                        },
                    }

                    split_buffer.existing_pieces.insert(0);
                },
                Type::UNRELIABLE_SPLIT => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let split_id: u16 = seek_read!(read_cursor.read_varint(), "split_id");
                    let count: usize = seek_read!(read_cursor.read_varint(), "count");

                    packet.start += read_cursor.position() as usize;
                    let packet: &[u8] = packet.as_ref();

                    let split_buffer = match self
                        .unreliable_split_buffers
                        .get_mut(&channel)
                        .filter(|b| b.split_id == split_id)
                    {
                        Some(b) => b,
                        None => continue,
                    };

                    match split_buffer.buffer.get_mut(&count) {
                        Some((current_length, shard)) => {
                            (&mut shard[.. packet.len()]).copy_from_slice(&packet);
                            *current_length = packet.len();
                        },
                        None => {
                            let mut new_shard = [0u8; MAX_DATA_SIZE];
                            (&mut new_shard[.. packet.len()]).copy_from_slice(&packet);
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

                        return Ok((channel, buf.into()));
                    }
                },
                Type::RELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    // TODO: do not answer if the sequence is not previous, but random?
                    stream_send_ack(&self.cipher, sender, sequence, &mut self.transport_sender)
                        .await?;

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

                            return Ok((
                                channel,
                                mem::replace(&mut self.split_buffer, Vec::new()).into(),
                            ));
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
                    stream_send_ack(&self.cipher, sender, sequence, &mut self.transport_sender)
                        .await?;

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
    address: SocketAddr,
    ack_sender: ChannelTx<AckBuf>,
    in_queue: ChannelTx<TypedBuffer>,
}

struct Clients {
    clients: Vec<Option<Client>>,
    free_ids: VecDeque<Id>,
}

impl Clients {
    fn new() -> Self {
        Self {
            clients: Vec::new(),
            free_ids: VecDeque::new(),
        }
    }

    fn get(&self, id: Id) -> Option<&Client> {
        self.clients.get(id)?.as_ref()
    }

    fn get_mut(&mut self, id: Id) -> Option<&mut Client> {
        self.clients.get_mut(id)?.as_mut()
    }

    fn push(&mut self, client: Client) -> usize {
        match self.free_ids.pop_front() {
            Some(id) => {
                *self.clients.get_mut(id).unwrap() = Some(client);
                id
            },
            None => {
                self.clients.push(Some(client));
                self.clients.len() - 1
            },
        }
    }

    fn remove(&mut self, id: Id) -> Option<Client> {
        let res = mem::replace(self.clients.get_mut(id)?, None);

        if res.is_some() {
            self.free_ids.push_back(id);
        }

        res
    }
}

enum ServerPacket {
    In((usize, SocketAddr)),
    Out((Id, Buffer, OneshotTx<Result<(), Error>>)),
}

struct AckBuf {
    buffer: [u8; ACKNOWLEDGE_SIZE],
    tag_start: usize,
    length: usize,
}

pub struct Connection {
    pub self_key: Key,
    pub peer_key: Key,
    pub sender: StreamSender,
    pub receiver: StreamReceiver,
}

pub struct Server {
    clients: Clients,
    out_queue: ChannelRx<(Id, Buffer, OneshotTx<Result<(), Error>>)>,
    out_queue_sender: ChannelTx<(Id, Buffer, OneshotTx<Result<(), Error>>)>,
    receive_buffer: [u8; MAX_PACKET_SIZE],
    transport: Async<UdpSocket>,
}

impl Server {
    pub fn bind<A>(bind_address: A) -> Result<Self, StdIoError>
    where
        A: Into<SocketAddr>,
    {
        let transport = Async::<UdpSocket>::bind(bind_address.into())?;
        let (out_queue_sender, out_queue) = new_channel();
        Ok(Self {
            clients: Clients::new(),
            out_queue,
            out_queue_sender,
            receive_buffer: [0; MAX_PACKET_SIZE],
            transport,
        })
    }

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
            .race(async {
                Ok(ServerPacket::In(
                    self.transport.recv_from(&mut self.receive_buffer).await?,
                ))
            })
            .await;

            match next? {
                ServerPacket::In((len, addr)) => {
                    let mut read_cursor = Cursor::new(&self.receive_buffer);
                    let sender: usize = seek_read!(read_cursor.read_varint(), "sender");

                    let mut packet_type = Type::UNDEFINED;
                    seek_read!(read_cursor.read(slice::from_mut(&mut packet_type)), "type");

                    match packet_type {
                        Type::CONNECT => {
                            let (in_queue_tx, in_queue_rx) = new_channel();

                            let (ack_sender, ack_receiver) = new_channel();

                            let client = Client {
                                address: addr,
                                ack_sender,
                                in_queue: in_queue_tx,
                            };

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
                            let self_key: Key = EncodedPoint::from(keypair.public_key())
                                .as_bytes()
                                .try_into()
                                .unwrap();

                            let id = self.clients.push(client);

                            let mut write_cursor = Cursor::new(self.receive_buffer.as_mut());

                            seek_write!(write_cursor.write_varint(SERVER_ID), "sender");
                            seek_write!(write_cursor.write_varint(Type::ACCEPT), "type");
                            seek_write!(write_cursor.write_all(&self_key), "key");
                            seek_write!(write_cursor.write_varint(id), "id");

                            self.transport.send_to(write_cursor.slice(), addr).await?;

                            let cipher = ChaCha20Poly1305::new((&secret).into());

                            return Ok(Connection {
                                self_key,
                                peer_key,
                                sender: StreamSender {
                                    unreliable: StreamUnreliableSender {
                                        peer: id,
                                        cipher: cipher.clone(),
                                        unreliable_split_id: 0,
                                        transport: self.out_queue_sender.clone(),
                                    },
                                    reliable: StreamReliableSender {
                                        peer: id,
                                        cipher: cipher.clone(),
                                        sequence: 0,
                                        ack_receiver,
                                        transport: self.out_queue_sender.clone(),
                                    },
                                },
                                receiver: StreamReceiver {
                                    cipher,
                                    sequence: 0,
                                    split_buffer: Vec::new(),
                                    split_channel: None,
                                    unreliable_split_buffers: HashMap::new(),
                                    transport_sender: self.out_queue_sender.clone(),
                                    transport_receiver: in_queue_rx,
                                },
                            });
                        },
                        Type::ACKNOWLEDGE => {
                            let mut remove = false;

                            if len > ACKNOWLEDGE_SIZE {
                                continue;
                            }

                            if let Some(client) = self.clients.get_mut(sender) {
                                let mut buffer = [0; ACKNOWLEDGE_SIZE];
                                buffer.copy_from_slice(&self.receive_buffer[.. ACKNOWLEDGE_SIZE]);
                                if client
                                    .ack_sender
                                    .send(AckBuf {
                                        buffer,
                                        tag_start: read_cursor.position() as usize,
                                        length: len,
                                    })
                                    .is_err()
                                {
                                    #[cfg(feature = "single")]
                                    {
                                        remove = client.in_queue.has_receiver();
                                    }
                                    #[cfg(feature = "multi")]
                                    {
                                        remove = !client.in_queue.is_disconnected();
                                    }
                                } else {
                                    client.address = addr;
                                }
                            }

                            if remove {
                                self.clients.remove(sender);
                            }
                        },
                        _ => {
                            let mut remove = false;

                            if let Some(client) = self.clients.get_mut(sender) {
                                let start = read_cursor.position() as usize;
                                let data = TypedBuffer {
                                    sender,
                                    packet_type,
                                    packet: Buffer {
                                        buffer: mem::replace(
                                            &mut self.receive_buffer,
                                            [0; MAX_PACKET_SIZE],
                                        ),
                                        start,
                                        stop: len,
                                    },
                                };
                                if client.in_queue.send(data).is_err() {
                                    #[cfg(feature = "single")]
                                    {
                                        remove = client.in_queue.has_receiver();
                                    }
                                    #[cfg(feature = "multi")]
                                    {
                                        remove = !client.in_queue.is_disconnected();
                                    }
                                } else {
                                    client.address = addr;
                                }
                            }

                            if remove {
                                self.clients.remove(sender);
                            }
                        },
                    }
                },
                ServerPacket::Out((id, packet, res_sender)) => {
                    #[cfg(feature = "multi")]
                    let mut res_sender = res_sender;

                    let client = match self.clients.get(id) {
                        Some(c) => c,
                        None => {
                            let _ = res_sender.send(Err(Error::InvalidConnection));
                            continue;
                        },
                    };

                    if let Err(err) = self
                        .transport
                        .send_to(packet.as_ref(), client.address)
                        .await
                    {
                        let _ = res_sender.send(Err(err.into()));
                    } else {
                        let _ = res_sender.send(Ok(()));
                    }
                },
            }
        }
    }
}
