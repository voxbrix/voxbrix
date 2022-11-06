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
    MAX_DATA_SIZE,
    MAX_PACKET_SIZE,
    SERVER_ID,
};
use async_io::{
    Async,
    Timer,
};
use futures_lite::future::FutureExt;
use integer_encoding::{
    VarIntReader,
    VarIntWriter,
};
use local_channel::mpsc::{
    Receiver as ChannelRx,
    Sender as ChannelTx,
};
use log::warn;
use std::{
    collections::VecDeque,
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
    slice,
    time::Duration,
};

// pub type Packet = Vec<u8>;

// pub type Data = Vec<u8>;

struct TypedBuffer {
    sender: Id,
    packet_type: u8,
    packet: Buffer,
}

async fn stream_send_ack(
    peer: Id,
    sequence: Sequence,
    transport: &mut ChannelTx<(Id, Buffer, ChannelTx<Result<(), StdIoError>>)>,
) -> Result<(), StdIoError> {
    let mut buffer = [0; MAX_PACKET_SIZE];

    let mut cursor = Cursor::new(buffer.as_mut());

    cursor
        .write_varint(SERVER_ID)
        .map_err(|_| StdIoErrorKind::OutOfMemory)?;
    cursor
        .write_varint(Type::ACKNOWLEDGE)
        .map_err(|_| StdIoErrorKind::OutOfMemory)?;
    cursor
        .write_varint(sequence)
        .map_err(|_| StdIoErrorKind::OutOfMemory)?;

    let stop = cursor.position() as usize;

    let (result_tx, mut result_rx) = local_channel::mpsc::channel();

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
        .map_err(|_| StdIoErrorKind::BrokenPipe)?;

    result_rx
        .recv()
        .await
        .ok_or_else(|| StdIoErrorKind::BrokenPipe)??;

    Ok(())
}

pub struct StreamSender {
    peer: Id,
    sequence: Sequence,
    ack_receiver: ChannelRx<Sequence>,
    transport: ChannelTx<(Id, Buffer, ChannelTx<Result<(), StdIoError>>)>,
}

impl StreamSender {
    pub async fn send_unreliable(&self, channel: usize, data: &[u8]) -> Result<(), StdIoError> {
        let mut buffer = [0; MAX_PACKET_SIZE];

        let mut cursor = Cursor::new(buffer.as_mut());

        cursor.write_varint(SERVER_ID).unwrap();
        cursor.write_varint(Type::UNRELIABLE).unwrap();
        cursor.write_varint(channel).unwrap();
        cursor.write_all(&data)?;

        let stop = cursor.position() as usize;

        let (result_tx, mut result_rx) = local_channel::mpsc::channel();

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
            .map_err(|_| StdIoErrorKind::BrokenPipe)?;

        result_rx
            .recv()
            .await
            .ok_or_else(|| StdIoErrorKind::BrokenPipe)??;

        Ok(())
    }

    async fn send_reliable_one(
        &mut self,
        channel: usize,
        data: &[u8],
        packet_type: u8,
    ) -> Result<(), StdIoError> {
        loop {
            let mut buffer = [0; MAX_PACKET_SIZE];

            let mut cursor = Cursor::new(buffer.as_mut());

            cursor.write_varint(SERVER_ID)?;
            cursor.write_varint(packet_type)?;
            cursor.write_varint(channel)?;
            cursor.write_varint(self.sequence)?;
            cursor.write_all(data)?;

            let stop = cursor.position() as usize;

            let (result_tx, mut result_rx) = local_channel::mpsc::channel();

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
                .map_err(|_| StdIoErrorKind::BrokenPipe)?;

            result_rx
                .recv()
                .await
                .ok_or_else(|| StdIoErrorKind::BrokenPipe)??;

            let result: Result<_, StdIoError> = async {
                while let Some(ack) = self.ack_receiver.recv().await {
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
                    self.sequence += 1;
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

pub struct StreamReceiver {
    sequence: Sequence,
    split_buffer: Vec<u8>,
    split_channel: Option<Channel>,
    transport_sender: ChannelTx<(Id, Buffer, ChannelTx<Result<(), StdIoError>>)>,
    transport_receiver: ChannelRx<TypedBuffer>,
}

impl StreamReceiver {
    pub async fn recv<'a>(&mut self) -> Result<(Channel, Packet), StdIoError> {
        loop {
            let TypedBuffer {
                sender,
                packet_type,
                mut packet,
            } = self
                .transport_receiver
                .recv()
                .await
                .ok_or_else(|| StdIoErrorKind::BrokenPipe)?;

            let mut read_cursor = Cursor::new(packet.as_ref());

            match packet_type {
                Type::DISCONNECT => {
                    return Err(StdIoErrorKind::NotConnected.into());
                },
                Type::PING => {
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    stream_send_ack(sender, sequence, &mut self.transport_sender).await?;
                },
                Type::UNRELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    packet.start = read_cursor.position() as usize;
                    return Ok((channel, packet.into()));
                },
                Type::RELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    stream_send_ack(sender, sequence, &mut self.transport_sender).await?;

                    if sequence == self.sequence {
                        self.sequence += 1;

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

                    stream_send_ack(sender, sequence, &mut self.transport_sender).await?;

                    if sequence == self.sequence {
                        self.sequence += 1;
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
    ack_sender: ChannelTx<Sequence>,
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
    Out((Id, Buffer, ChannelTx<Result<(), StdIoError>>)),
}

pub struct Server {
    clients: Clients,
    out_queue: ChannelRx<(Id, Buffer, ChannelTx<Result<(), StdIoError>>)>,

    // To prevent [1] from panic
    out_queue_sender: ChannelTx<(Id, Buffer, ChannelTx<Result<(), StdIoError>>)>,

    receive_buffer: [u8; MAX_PACKET_SIZE],
    transport: Async<UdpSocket>,
}

impl Server {
    pub fn bind<A>(bind_address: A) -> Result<Self, StdIoError>
    where
        A: Into<SocketAddr>,
    {
        let transport = Async::<UdpSocket>::bind(bind_address.into())?;
        let (out_queue_sender, out_queue) = local_channel::mpsc::channel();
        Ok(Self {
            clients: Clients::new(),
            out_queue,
            out_queue_sender,
            receive_buffer: [0; MAX_PACKET_SIZE],
            transport,
        })
    }

    pub async fn accept(&mut self) -> Result<(StreamSender, StreamReceiver), StdIoError> {
        loop {
            let next: Result<_, StdIoError> = async {
                Ok(ServerPacket::Out(self.out_queue.recv().await.unwrap())) // [1]
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
                            let (in_queue_tx, in_queue_rx) = local_channel::mpsc::channel();

                            let (ack_sender, ack_receiver) = local_channel::mpsc::channel();

                            let client = Client {
                                address: addr,
                                ack_sender,
                                in_queue: in_queue_tx,
                            };

                            let id = self.clients.push(client);

                            let mut write_cursor = Cursor::new(self.receive_buffer.as_mut());

                            seek_write!(write_cursor.write_varint(SERVER_ID), "sender");
                            seek_write!(write_cursor.write_varint(Type::ASSIGN_ID), "type");
                            seek_write!(write_cursor.write_varint(id), "id");

                            self.transport.send_to(write_cursor.slice(), addr).await?;

                            return Ok((
                                StreamSender {
                                    peer: id,
                                    sequence: 0,
                                    ack_receiver,
                                    transport: self.out_queue_sender.clone(),
                                },
                                StreamReceiver {
                                    sequence: 0,
                                    split_buffer: Vec::new(),
                                    split_channel: None,
                                    transport_sender: self.out_queue_sender.clone(),
                                    transport_receiver: in_queue_rx,
                                },
                            ));
                        },
                        Type::ACKNOWLEDGE => {
                            let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                            let mut remove = false;

                            if let Some(client) = self.clients.get_mut(sender) {
                                if client.ack_sender.send(sequence).is_err() {
                                    remove = true;
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
                                    remove = true;
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
                    let client = match self.clients.get(id) {
                        Some(c) => c,
                        None => {
                            let _ = res_sender.send(Err(StdIoErrorKind::NotConnected.into()));
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
