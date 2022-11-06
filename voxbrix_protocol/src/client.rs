use super::{
    seek_read,
    seek_write,
    AsSlice,
    Channel,
    Id,
    Sequence,
    Type,
    // 508 - channel(16) - sender(16) - assign_id(16)
    MAX_DATA_SIZE,

    MAX_PACKET_SIZE,
    NEW_CONNECTION_ID,

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

        let (ack_sender, ack_receiver) = local_channel::mpsc::channel();

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
            split_buffer: Vec::new(),
            split_channel: None,
            buffer: [0; MAX_PACKET_SIZE],
            ack_sender,
            transport: transport.clone(),
        };

        let sender = Sender {
            id,
            sequence: 0,
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
    split_buffer: Vec<u8>,
    split_channel: Option<Channel>,
    buffer: [u8; MAX_PACKET_SIZE],
    ack_sender: ChannelTx<Sequence>,
    transport: T,
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
                Type::PING => {
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    let mut write_cursor = Cursor::new(self.buffer.as_mut());

                    seek_write!(write_cursor.write_varint(self.id), "sender");
                    seek_write!(write_cursor.write_varint(Type::ACKNOWLEDGE), "type");
                    seek_write!(write_cursor.write_varint(sequence), "sequence");

                    self.transport.send(write_cursor.slice()).await?;
                },
                Type::UNRELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let start = read_cursor.position() as usize;
                    return Ok((channel, &mut buf[start .. len]));
                },
                Type::RELIABLE => {
                    let channel: usize = seek_read!(read_cursor.read_varint(), "channel");
                    let sequence: u16 = seek_read!(read_cursor.read_varint(), "sequence");

                    let mut write_cursor = Cursor::new(self.buffer.as_mut());

                    seek_write!(write_cursor.write_varint(self.id), "sender");
                    seek_write!(write_cursor.write_varint(Type::ACKNOWLEDGE), "type");
                    seek_write!(write_cursor.write_varint(sequence), "sequence");

                    self.transport.send(write_cursor.slice()).await?;

                    if sequence == self.sequence {
                        self.sequence += 1;
                        let start = read_cursor.position() as usize;

                        if self.split_channel.is_none() {
                            return Ok((channel, &mut buf[start .. len]));
                        } else {
                            self.split_buffer.extend_from_slice(&mut buf[start .. len]);

                            mem::swap(&mut self.split_buffer, buf);

                            self.split_buffer.clear();
                            self.split_channel = None;

                            return Ok((channel, buf));
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

                    let mut write_cursor = Cursor::new(self.buffer.as_mut());

                    seek_write!(write_cursor.write_varint(self.id), "sender");
                    seek_write!(write_cursor.write_varint(Type::ACKNOWLEDGE), "type");
                    seek_write!(write_cursor.write_varint(sequence), "sequence");

                    self.transport.send(write_cursor.slice()).await?;

                    if sequence == self.sequence {
                        self.sequence += 1;
                        let start = read_cursor.position() as usize;

                        self.split_buffer.extend_from_slice(&mut buf[start .. len]);
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
    buffer: [u8; MAX_PACKET_SIZE],
    ack_receiver: ChannelRx<Sequence>,
    transport: T,
}

impl<T> Sender<T>
where
    T: Deref<Target = Async<UdpSocket>>,
{
    pub async fn send_unreliable(&self, channel: usize, data: &[u8]) -> Result<(), StdIoError> {
        let mut buffer = [0u8; MAX_PACKET_SIZE];

        let mut write_cursor = Cursor::new(buffer.as_mut());

        write_cursor.write_varint(self.id).unwrap();
        write_cursor.write_varint(Type::UNRELIABLE).unwrap();
        write_cursor.write_varint(channel).unwrap();
        write_cursor
            .write_all(&data)
            .map_err(|_| StdIoErrorKind::OutOfMemory)?;

        self.transport.send(write_cursor.slice()).await?;

        Ok(())
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
