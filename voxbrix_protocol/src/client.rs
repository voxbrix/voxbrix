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
use async_io::{
    Async,
    Timer,
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
#[cfg(feature = "single")]
use local_channel::mpsc::{
    channel as new_channel,
    Receiver as ChannelRx,
    Sender as ChannelTx,
};
use log::warn;
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
        HashMap,
    },
    io::{
        Cursor,
        Error as StdIoError,
        ErrorKind as StdIoErrorKind,
        Read,
        Write,
    },
    mem,
    net::{
        SocketAddr,
        UdpSocket,
    },
    ops::Deref,
    rc::Rc,
    slice,
    time::Duration,
};

pub struct Client {
    transport: Async<UdpSocket>,
}

impl Client {
    pub fn bind<A>(bind_address: A) -> Result<Self, StdIoError>
    where
        A: Into<SocketAddr>,
    {
        let transport = Async::<UdpSocket>::bind(bind_address.into())?;
        Ok(Self { transport })
    }

    pub async fn connect<A>(
        self,
        server_address: A,
    ) -> Result<(Sender<Rc<Async<UdpSocket>>>, Receiver<Rc<Async<UdpSocket>>>), StdIoError>
    where
        A: Into<SocketAddr>,
    {
        self.transport.get_ref().connect(server_address.into())?;

        let transport = Rc::new(self.transport);

        let (ack_sender, ack_receiver) = new_channel();

        let mut buf = [0u8; MAX_PACKET_SIZE];

        let mut write_cursor = Cursor::new(buf.as_mut());

        write_cursor.write_varint(NEW_CONNECTION_ID).unwrap();
        write_cursor.write_varint(Type::CONNECT).unwrap();

        transport.send(write_cursor.slice()).await?;

        let id = loop {
            let len = transport.recv(&mut buf).await?;

            let mut read_cursor = Cursor::new(&buf[.. len]);

            let sender: usize = seek_read!(read_cursor.read_varint(), "sender");

            if sender != SERVER_ID {
                continue;
            }

            let mut packet_type = Type::UNDEFINED;
            seek_read!(read_cursor.read(slice::from_mut(&mut packet_type)), "type");

            match packet_type {
                Type::ASSIGN_ID => {
                    let id: usize = seek_read!(read_cursor.read_varint(), "id");

                    break id;
                },
                _ => {},
            }
        };

        let receiver = Receiver {
            id,
            sequence: 0,
            reliable_split_buffer: Vec::new(),
            reliable_split_channel: None,
            buffer: [0; MAX_PACKET_SIZE],
            ack_sender,
            transport: transport.clone(),
            unreliable_split_buffers: HashMap::new(),
        };

        let sender = Sender {
            id,
            sequence: 0,
            unreliable_split_id: 0,
            buffer: [0; MAX_PACKET_SIZE],
            ack_receiver,
            transport: transport.clone(),
        };

        Ok((sender, receiver))
    }
}

pub struct Receiver<T> {
    id: Id,
    sequence: Sequence,
    reliable_split_buffer: Vec<u8>,
    reliable_split_channel: Option<Channel>,
    buffer: [u8; MAX_PACKET_SIZE],
    ack_sender: ChannelTx<Sequence>,
    transport: T,
    unreliable_split_buffers: HashMap<Channel, UnreliableBuffer>,
}

impl<T> Receiver<T>
where
    T: Deref<Target = Async<UdpSocket>>,
{
    pub async fn recv<'a>(
        &mut self,
        buf: &'a mut Vec<u8>,
    ) -> Result<(Channel, &'a mut [u8]), StdIoError> {
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

            match packet_type {
                Type::ACKNOWLEDGE => {
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");
                    let _ = self.ack_sender.send(sequence);
                },
                Type::DISCONNECT => {
                    return Err(StdIoErrorKind::NotConnected.into());
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

                        // TODO: also check CRC and if it's incorrect restore buf length to
                        // MAX_PACKET_SIZE before continuing

                        return Ok((channel, buf.as_mut()));
                    }
                },
                Type::RELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    // TODO: do not answer if the sequence is not previous, but random?
                    let mut write_cursor = Cursor::new(self.buffer.as_mut());

                    seek_write!(write_cursor.write_varint(self.id), "sender");
                    seek_write!(write_cursor.write_varint(Type::ACKNOWLEDGE), "type");
                    seek_write!(write_cursor.write_varint(sequence), "sequence");

                    self.transport.send(write_cursor.slice()).await?;

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
                    let mut write_cursor = Cursor::new(self.buffer.as_mut());

                    seek_write!(write_cursor.write_varint(self.id), "sender");
                    seek_write!(write_cursor.write_varint(Type::ACKNOWLEDGE), "type");
                    seek_write!(write_cursor.write_varint(sequence), "sequence");

                    self.transport.send(write_cursor.slice()).await?;

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

pub struct Sender<T> {
    id: Id,
    sequence: Sequence,
    unreliable_split_id: u16,
    buffer: [u8; MAX_PACKET_SIZE],
    ack_receiver: ChannelRx<Sequence>,
    transport: T,
}

impl<T> Sender<T>
where
    T: Deref<Target = Async<UdpSocket>>,
{
    async fn send_unreliable_one(
        &self,
        channel: usize,
        data: &[u8],
        message_type: u8,
        len_or_count: Option<usize>,
    ) -> Result<(), StdIoError> {
        let mut buffer = [0; MAX_PACKET_SIZE];

        let mut cursor = Cursor::new(buffer.as_mut());

        cursor.write_varint(self.id).unwrap();
        cursor.write_varint(message_type).unwrap();
        cursor.write_varint(channel).unwrap();
        if let Some(len_or_count) = len_or_count {
            cursor.write_varint(self.unreliable_split_id).unwrap();
            cursor.write_varint(len_or_count).unwrap();
        }
        cursor.write_all(&data)?;

        self.transport.send(cursor.slice()).await?;

        Ok(())
    }

    pub async fn send_unreliable(&mut self, channel: usize, data: &[u8]) -> Result<(), StdIoError> {
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

    async fn send_reliable_one(
        &mut self,
        channel: usize,
        data: &[u8],
        packet_type: u8,
    ) -> Result<(), StdIoError> {
        loop {
            let mut write_cursor = Cursor::new(self.buffer.as_mut());

            write_cursor.write_varint(self.id).unwrap();
            write_cursor.write_varint(packet_type).unwrap();
            write_cursor.write_varint(channel).unwrap();
            write_cursor.write_varint(self.sequence).unwrap();
            write_cursor
                .write_all(data)
                .map_err(|_| StdIoErrorKind::OutOfMemory)?;

            self.transport.send(write_cursor.slice()).await?;

            let result: Result<_, StdIoError> = async {
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

                Err(StdIoErrorKind::BrokenPipe.into())
            }
            .or(async {
                Timer::after(Duration::from_secs(1)).await;
                Err(StdIoErrorKind::TimedOut.into())
            })
            .await;

            match result {
                Ok(()) => {
                    self.sequence = self.sequence.wrapping_add(1);
                    return Ok(());
                },
                Err(err) if err.kind() == StdIoErrorKind::BrokenPipe => return Err(err),
                _ => {},
            }
        }
    }

    pub async fn send_reliable(&mut self, channel: usize, data: &[u8]) -> Result<(), StdIoError> {
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
