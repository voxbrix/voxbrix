use super::{
    seek_read,
    seek_write,
    AsSlice,
    Channel,
    Id,
    Sequence,
    Type,
    UnreliableBuffer,
    MAX_DATA_SIZE,
    MAX_PACKET_SIZE,
    NEW_CONNECTION_ID,
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
use local_channel::mpsc::{
    channel as new_channel,
    Receiver as ChannelRx,
    Sender as ChannelTx,
};
use log::warn;
use rand_core::OsRng;
#[cfg(feature = "single")]
use std::rc::Rc;
#[cfg(feature = "multi")]
use std::sync::Arc;
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
        HashMap,
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
    ReceiverWasDropped,
    Disconnect,
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

pub struct Client {
    transport: Async<UdpSocket>,
}

pub struct Connection {
    pub self_key: Key,
    pub peer_key: Key,
    pub sender: Sender,
    pub receiver: Receiver,
}

impl Client {
    pub fn bind<A>(bind_address: A) -> Result<Self, Error>
    where
        A: Into<SocketAddr>,
    {
        let transport = Async::<UdpSocket>::bind(bind_address.into())?;
        Ok(Self { transport })
    }

    pub async fn connect<A>(self, server_address: A) -> Result<Connection, Error>
    where
        A: Into<SocketAddr>,
    {
        self.transport.get_ref().connect(server_address.into())?;

        #[cfg(feature = "single")]
        let transport = Rc::new(self.transport);

        #[cfg(feature = "multi")]
        let transport = Arc::new(self.transport);

        let (ack_sender, ack_receiver) = new_channel();

        let keypair = EphemeralSecret::random(&mut OsRng);
        let self_key: Key = EncodedPoint::from(keypair.public_key())
            .as_ref()
            .try_into()
            .unwrap();

        let mut buf = [0u8; MAX_PACKET_SIZE];

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
                continue;
            }

            let mut packet_type = Type::UNDEFINED;
            seek_read!(read_cursor.read(slice::from_mut(&mut packet_type)), "type");

            match packet_type {
                Type::ACCEPT => {
                    let mut key = KEY_BUFFER;

                    seek_read!(read_cursor.read_exact(&mut key), "peer key");
                    let id: usize = seek_read!(read_cursor.read_varint(), "id");

                    let deciphered_peer_key =
                        seek_read!(PublicKey::from_sec1_bytes(&key), "deciphered peer key");

                    break (key, deciphered_peer_key, id);
                },
                _ => {},
            }
        };

        let mut secret = SECRET_BUFFER;
        keypair
            .diffie_hellman(&deciphered_peer_key)
            .extract::<sha2::Sha256>(None)
            .expand(&[], &mut secret)
            .unwrap();

        let cipher = ChaCha20Poly1305::new((&secret).into());

        let receiver = Receiver {
            id,
            cipher: cipher.clone(),
            sequence: 0,
            reliable_split_buffer: Vec::new(),
            reliable_split_channel: None,
            buffer: [0; MAX_PACKET_SIZE],
            ack_sender,
            transport: transport.clone(),
            unreliable_split_buffers: HashMap::new(),
        };

        let sender = Sender {
            unreliable: UnreliableSender {
                id,
                cipher: cipher.clone(),
                unreliable_split_id: 0,
                transport: transport.clone(),
            },
            reliable: ReliableSender {
                id,
                cipher,
                sequence: 0,
                buffer: [0; MAX_PACKET_SIZE],
                ack_receiver,
                transport,
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

pub struct Receiver {
    id: Id,
    cipher: ChaCha20Poly1305,
    sequence: Sequence,
    reliable_split_buffer: Vec<u8>,
    reliable_split_channel: Option<Channel>,
    buffer: [u8; MAX_PACKET_SIZE],
    ack_sender: ChannelTx<Sequence>,

    #[cfg(feature = "single")]
    transport: Rc<Async<UdpSocket>>,

    #[cfg(feature = "multi")]
    transport: Arc<Async<UdpSocket>>,
    unreliable_split_buffers: HashMap<Channel, UnreliableBuffer>,
}

impl Receiver {
    async fn send_ack(&mut self, sequence: Sequence) -> Result<(), Error> {
        let (tag_start, len) =
            crate::write_in_buffer(&mut self.buffer, self.id, Type::ACKNOWLEDGE, |cursor| {
                cursor.write_varint(sequence).unwrap();
            });

        crate::encode_in_buffer(&mut self.buffer, &self.cipher, tag_start, len);

        self.transport.send(&self.buffer[.. len]).await?;
        Ok(())
    }

    pub async fn recv<'a>(
        &mut self,
        buf: &'a mut Vec<u8>,
    ) -> Result<(Channel, &'a mut [u8]), Error> {
        loop {
            buf.resize(MAX_PACKET_SIZE, 0);

            let len = self.transport.recv(buf).await?;

            let mut read_cursor = Cursor::new(&buf[.. len]);

            let sender: usize = seek_read!(read_cursor.read_varint(), "sender");

            if sender != SERVER_ID {
                continue;
            }

            let mut packet_type = Type::UNDEFINED;
            seek_read!(read_cursor.read(slice::from_mut(&mut packet_type)), "type");

            let tag_start = read_cursor.position() as usize;

            let decrypted_start =
                match crate::decode_in_buffer(&mut buf[.. len], tag_start, &self.cipher) {
                    Ok(s) => s,
                    Err(()) => continue,
                };

            let mut read_cursor = Cursor::new(&buf[.. len]);
            read_cursor.set_position(decrypted_start as u64);

            match packet_type {
                Type::ACKNOWLEDGE => {
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");
                    let _ = self.ack_sender.send(sequence);
                },
                Type::DISCONNECT => {
                    return Err(Error::Disconnect);
                },
                Type::UNRELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let start = read_cursor.position() as usize;
                    return Ok((channel, &mut buf[start .. len]));
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

                    let start = read_cursor.position() as usize;

                    let data_length = len - start;

                    match split_buffer.buffer.get_mut(&0) {
                        Some((current_length, shard)) => {
                            (&mut shard[.. data_length]).copy_from_slice(&buf[start .. len]);
                            *current_length = data_length;
                        },
                        None => {
                            let mut new_shard = [0u8; MAX_DATA_SIZE];
                            (&mut new_shard[.. data_length]).copy_from_slice(&buf[start .. len]);
                            split_buffer.buffer.insert(0, (data_length, new_shard));
                        },
                    }

                    split_buffer.existing_pieces.insert(0);
                },
                Type::UNRELIABLE_SPLIT => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let split_id: u16 = seek_read!(read_cursor.read_varint(), "split_id");
                    let count: usize = seek_read!(read_cursor.read_varint(), "count");
                    let start = read_cursor.position() as usize;
                    let data_length = len - start;

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
                            (&mut shard[.. data_length]).copy_from_slice(&buf[start .. len]);
                            *current_length = data_length;
                        },
                        None => {
                            let mut new_shard = [0u8; MAX_DATA_SIZE];
                            (&mut new_shard[.. data_length]).copy_from_slice(&buf[start .. len]);
                            split_buffer.buffer.insert(count, (data_length, new_shard));
                        },
                    }

                    split_buffer.existing_pieces.insert(count);

                    if split_buffer
                        .existing_pieces
                        .range(0 .. split_buffer.expected_length)
                        .count()
                        == split_buffer.expected_length
                    {
                        buf.clear();

                        for (_, (len, data)) in
                            split_buffer.buffer.range(0 .. split_buffer.expected_length)
                        {
                            buf.extend_from_slice(&data[.. *len]);
                        }

                        return Ok((channel, buf.as_mut()));
                    }
                },
                Type::RELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    // TODO: do not answer if the sequence is not previous, but random?
                    seek_write!(self.send_ack(sequence).await, "ack message");

                    if sequence == self.sequence {
                        self.sequence = self.sequence.wrapping_add(1);
                        let start = read_cursor.position() as usize;

                        if self.reliable_split_channel.is_none() {
                            return Ok((channel, &mut buf[start .. len]));
                        } else {
                            self.reliable_split_buffer
                                .extend_from_slice(&mut buf[start .. len]);

                            mem::swap(&mut self.reliable_split_buffer, buf);

                            self.reliable_split_buffer.clear();
                            self.reliable_split_channel = None;

                            return Ok((channel, buf));
                        }
                    }
                },
                Type::RELIABLE_SPLIT => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");

                    if self.reliable_split_channel.is_none() {
                        self.reliable_split_channel = Some(channel);
                    } else if let Some(reliable_split_channel) = self.reliable_split_channel {
                        if reliable_split_channel != channel {
                            warn!("skipping mishappened packet with channel {}", channel);
                            continue;
                        }
                    }

                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    // TODO: do not answer if the sequence is not previous, but random?
                    seek_write!(self.send_ack(sequence).await, "ack message");

                    if sequence == self.sequence {
                        self.sequence = self.sequence.wrapping_add(1);
                        let start = read_cursor.position() as usize;

                        self.reliable_split_buffer
                            .extend_from_slice(&mut buf[start .. len]);
                    }
                },
                _ => {},
            }
        }
    }
}

pub struct Sender {
    unreliable: UnreliableSender,
    reliable: ReliableSender,
}

impl Sender {
    pub async fn send_unreliable(&mut self, channel: usize, data: &[u8]) -> Result<(), Error> {
        self.unreliable.send_unreliable(channel, data).await
    }

    pub async fn send_reliable(&mut self, channel: usize, data: &[u8]) -> Result<(), Error> {
        self.reliable.send_reliable(channel, data).await
    }
}

impl Sender {
    pub fn split(self) -> (UnreliableSender, ReliableSender) {
        let Self {
            unreliable,
            reliable,
        } = self;

        (unreliable, reliable)
    }
}

pub struct UnreliableSender {
    id: Id,
    cipher: ChaCha20Poly1305,
    unreliable_split_id: u16,

    #[cfg(feature = "single")]
    transport: Rc<Async<UdpSocket>>,

    #[cfg(feature = "multi")]
    transport: Arc<Async<UdpSocket>>,
}

impl UnreliableSender {
    async fn send_unreliable_one(
        &self,
        channel: usize,
        data: &[u8],
        message_type: u8,
        len_or_count: Option<usize>,
    ) -> Result<(), Error> {
        let mut buffer = [0; MAX_PACKET_SIZE];

        let (tag_start, len) =
            crate::write_in_buffer(&mut buffer, self.id, message_type, |cursor| {
                cursor.write_varint(channel).unwrap();
                if let Some(len_or_count) = len_or_count {
                    cursor.write_varint(self.unreliable_split_id).unwrap();
                    cursor.write_varint(len_or_count).unwrap();
                }
                cursor.write_all(&data).unwrap();
            });

        crate::encode_in_buffer(&mut buffer, &self.cipher, tag_start, len);

        self.transport.send(&buffer[.. len]).await?;

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

pub struct ReliableSender {
    id: Id,
    cipher: ChaCha20Poly1305,
    sequence: Sequence,
    buffer: [u8; MAX_PACKET_SIZE],
    ack_receiver: ChannelRx<Sequence>,

    #[cfg(feature = "single")]
    transport: Rc<Async<UdpSocket>>,

    #[cfg(feature = "multi")]
    transport: Arc<Async<UdpSocket>>,
}

impl ReliableSender {
    async fn send_reliable_one(
        &mut self,
        channel: usize,
        data: &[u8],
        packet_type: u8,
    ) -> Result<(), Error> {
        let (tag_start, len) =
            crate::write_in_buffer(&mut self.buffer, self.id, packet_type, |cursor| {
                cursor.write_varint(channel).unwrap();
                cursor.write_varint(self.sequence).unwrap();
                cursor.write_all(data).unwrap();
            });

        crate::encode_in_buffer(&mut self.buffer, &self.cipher, tag_start, len);

        loop {
            self.transport.send(&self.buffer[.. len]).await?;

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

                Err(Some(Error::ReceiverWasDropped))
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
