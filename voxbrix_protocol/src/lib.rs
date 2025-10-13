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
use std::{
    io::{
        Cursor,
        Read,
        Result as IoResult,
        Write,
    },
    mem,
    time::Duration,
};

#[cfg(any(feature = "client"))]
pub mod client;

#[cfg(any(feature = "server"))]
pub mod server;

pub const MAX_PACKET_SIZE: usize = 508;

// Unreliable split type has the longest header:
const MAX_HEADER_SIZE: usize = mem::size_of::<Id>() // sender
    + 1 // type
    + TAG_SIZE // tag
    + NONCE_SIZE // nonce
    + mem::size_of::<Sequence>()
    + mem::size_of::<u32>(); // count/length

/// Maximum amount of data bytes that fits into one packet.
/// Unreliable messages sent are recommended to be smaller that this.
pub const MAX_DATA_SIZE: usize = MAX_PACKET_SIZE - MAX_HEADER_SIZE;
/// Maximum amount of data per message.
pub const MAX_SPLIT_DATA_SIZE: usize = MAX_SPLIT_PACKETS as usize * MAX_DATA_SIZE;

const SERVER_ID: Id = 0;
const NEW_CONNECTION_ID: Id = 1;
const UNRELIABLE_BUFFERS: usize = 8;
const RELIABLE_QUEUE_LENGTH: Sequence = 64;
const RELIABLE_RESEND_AFTER: Duration = Duration::from_millis(1000);
const MAX_SPLIT_PACKETS: u32 = 2000;

trait AsSlice<T> {
    fn slice(&self) -> &[T];
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

struct UnreliableBuffer<B> {
    start_sequence: Sequence,
    complete_shards: u32,
    shards: Vec<Option<B>>,
}

impl<B> UnreliableBuffer<B> {
    fn new(start_sequence: Sequence, expected_packets: u32) -> Self {
        let mut shards = Vec::new();
        shards.resize_with(expected_packets.to_usize(), || None);

        Self {
            start_sequence,
            complete_shards: 0,
            shards,
        }
    }

    fn is_complete(&self) -> bool {
        TryInto::<usize>::try_into(self.complete_shards).unwrap() == self.shards.len()
    }

    fn clear(&mut self, start_sequence: Sequence, expected_packets: u32) {
        self.start_sequence = start_sequence;
        self.complete_shards = 0;
        self.shards.clear();
        self.shards
            .resize_with(expected_packets.to_usize(), || None);
    }
}

#[macro_export]
macro_rules! seek_read {
    ($e:expr, $c:literal) => {
        match $e {
            Ok(r) => r,
            Err(_) => {
                log::debug!("read {} error", $c);
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
                log::debug!("read {} error", $c);
                return Err(());
            },
        }
    };
}

type Id = u32;
type Sequence = u128;
type Key = [u8; 33];
type Secret = [u8; 32];

const TYPE_INDEX: usize = mem::size_of::<Id>();
const TAG_START: usize = TYPE_INDEX + 1; // 1 is Type byte
const TAG_SIZE: usize = 16;
const NONCE_START: usize = TAG_START + TAG_SIZE;
const NONCE_SIZE: usize = 12;
const ENCRYPTED_START: usize = NONCE_START + NONCE_SIZE;

const KEY_BUFFER: Key = [0; 33];
const SECRET_BUFFER: Secret = [0; 32];
const TAG_BUFFER: [u8; TAG_SIZE] = [0; TAG_SIZE];
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
        // sequence: Sequence,
        // data: &[u8],

    const UNRELIABLE_SPLIT_START: u8 = 5;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // sequence: Sequence,
        // length: u32,
        // data: &[u8],

    const UNRELIABLE_SPLIT: u8 = 6;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // sequence: Sequence,
        // data: &[u8],

    const RELIABLE: u8 = 7;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // sequence: Sequence,
        // data: &[u8],

    const RELIABLE_SPLIT: u8 = 8;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // sequence: Sequence,
        // data: &[u8],

    const UNDEFINED: u8 = u8::MAX;
}

/// Returns tag start byte and total data length.
fn write_in_buffer<F>(
    buffer: &mut [u8; MAX_PACKET_SIZE],
    sender: Id,
    packet_type: u8,
    sequence: Sequence,
    mut f: F,
) -> usize
where
    F: FnMut(&mut Cursor<&mut [u8]>),
{
    let mut cursor = Cursor::new(buffer.as_mut());

    cursor.write_bytes(sender).unwrap();
    cursor.write_bytes(packet_type).unwrap();
    cursor.set_position(const { ENCRYPTED_START as u64 });
    cursor.write_bytes(sequence).unwrap();
    f(&mut cursor);

    cursor.position().to_usize()
}

fn encode_in_buffer(buffer: &mut [u8; MAX_PACKET_SIZE], cipher: &ChaCha20Poly1305, length: usize) {
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    buffer[NONCE_START .. ENCRYPTED_START].copy_from_slice(&nonce);

    let buffer = &mut buffer[.. length];

    let (buffer_pre_enc, buffer_enc) = buffer.split_at_mut(ENCRYPTED_START);

    let tag = cipher
        .encrypt_in_place_detached(&nonce, &buffer_pre_enc[.. TAG_START], buffer_enc)
        .unwrap();

    buffer[TAG_START .. NONCE_START].copy_from_slice(&tag);
}

/// Returns total data length.
fn tag_sign_in_buffer(buffer: &mut [u8; MAX_PACKET_SIZE], cipher: &ChaCha20Poly1305) {
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
    buffer[NONCE_START .. ENCRYPTED_START].copy_from_slice(&nonce);

    let tag = cipher
        .encrypt_in_place_detached(&nonce, &buffer[.. TAG_START], &mut [])
        .unwrap();

    buffer[TAG_START .. NONCE_START].copy_from_slice(&tag);
}

fn decode_in_buffer(buffer: &mut [u8], cipher: &ChaCha20Poly1305) -> Result<(), ()> {
    let mut cursor = Cursor::new(&buffer);
    cursor.set_position(TAG_START as u64);

    let mut tag = TAG_BUFFER;
    seek_read_return!(cursor.read_exact(&mut tag), "tag");

    let mut nonce = NONCE_BUFFER;
    seek_read_return!(cursor.read_exact(&mut nonce), "nonce");

    let (buffer_acc_data, buffer_encrypted) = {
        let (buffer, buffer_encrypted) = buffer.split_at_mut(ENCRYPTED_START);
        (&buffer[.. TAG_START], buffer_encrypted)
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

    Ok(())
}

impl AsFixedBytes for u8 {
    type Bytes = [u8; 1];

    fn to_bytes(self) -> Self::Bytes {
        [self]
    }

    fn from_bytes(bytes: Self::Bytes) -> Self {
        bytes[0]
    }

    fn zeroed() -> Self::Bytes {
        [0]
    }
}

impl AsFixedBytes for u32 {
    type Bytes = [u8; 4];

    fn to_bytes(self) -> Self::Bytes {
        self.to_le_bytes()
    }

    fn from_bytes(bytes: Self::Bytes) -> Self {
        Self::from_le_bytes(bytes)
    }

    fn zeroed() -> Self::Bytes {
        [0; mem::size_of::<Self::Bytes>()]
    }
}

impl AsFixedBytes for u128 {
    type Bytes = [u8; 16];

    fn to_bytes(self) -> Self::Bytes {
        self.to_le_bytes()
    }

    fn from_bytes(bytes: Self::Bytes) -> Self {
        Self::from_le_bytes(bytes)
    }

    fn zeroed() -> Self::Bytes {
        [0; mem::size_of::<Self::Bytes>()]
    }
}

trait AsFixedBytes {
    type Bytes: AsRef<[u8]> + AsMut<[u8]>;
    fn to_bytes(self) -> Self::Bytes;
    fn from_bytes(bytes: Self::Bytes) -> Self;
    fn zeroed() -> Self::Bytes;
}

trait WriteExt: Write {
    fn write_bytes(&mut self, n: impl AsFixedBytes) -> IoResult<usize> {
        self.write(n.to_bytes().as_ref())
    }
}

impl<T: Write> WriteExt for T {}

trait ReadExt: Read {
    fn read_bytes<T>(&mut self) -> IoResult<T>
    where
        T: AsFixedBytes,
    {
        let mut buf = T::zeroed();
        self.read_exact(buf.as_mut())?;

        Ok(T::from_bytes(buf))
    }
}

impl<T: Read> ReadExt for T {}

trait ToU128 {
    fn to_u128(self) -> u128;
}

impl ToU128 for u32 {
    fn to_u128(self) -> u128 {
        self.try_into().unwrap()
    }
}

impl ToU128 for usize {
    fn to_u128(self) -> u128 {
        self.try_into().unwrap()
    }
}

trait ToUsize {
    fn to_usize(self) -> usize;
}

impl ToUsize for u8 {
    fn to_usize(self) -> usize {
        self.try_into().unwrap()
    }
}

impl ToUsize for u32 {
    fn to_usize(self) -> usize {
        self.try_into().unwrap()
    }
}

impl ToUsize for u64 {
    fn to_usize(self) -> usize {
        self.try_into().unwrap()
    }
}

impl ToUsize for u128 {
    fn to_usize(self) -> usize {
        self.try_into().unwrap()
    }
}
