use crate::{
    component::block::metadata::BlockMetadata,
    entity::{
        actor::Actor,
        block::Block,
        block_class::BlockClass,
        block_environment::BlockEnvironment,
        chunk::Chunk,
        snapshot::{
            ClientSnapshot,
            ServerSnapshot,
        },
    },
    messages::{
        DispatchesPacked,
        UpdatesPacked,
    },
    pack::{
        self,
        Pack,
        Packer,
        UnpackError,
    },
};
use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};
use std::{
    marker::PhantomData,
    sync::Arc,
};

#[derive(Serialize, Deserialize)]
pub struct InitResponse {
    #[serde(with = "serde_big_array::BigArray")]
    pub public_key: [u8; 33],
    #[serde(with = "serde_big_array::BigArray")]
    pub key_signature: [u8; 64],
}

impl Pack for InitResponse {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize, Debug)]
pub enum LoginFailure {
    IncorrectCredentials,
    Unknown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RegisterFailure {
    UsernameTaken,
    Unknown,
}

#[derive(Serialize, Deserialize)]
pub struct InitData {
    pub actor: Actor,
    // position: Position,
}

impl Pack for InitData {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub enum LoginResult {
    Success(InitData),
    Failure(LoginFailure),
}

impl Pack for LoginResult {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub enum RegisterResult {
    Success(InitData),
    Failure(RegisterFailure),
}

impl Pack for RegisterResult {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub struct ChunkChanges<'a, T> {
    buffer: &'a [u8],
    _component: PhantomData<T>,
}

impl<'a, T> ChunkChanges<'a, T> {
    pub fn decode_chunks(self) -> Result<ChunkChangesChunkDecoder<'a, T>, UnpackError> {
        let (length, offset) = pack::decode_from_slice::<u64>(self.buffer).ok_or(UnpackError)?;

        let length = length.try_into().unwrap();

        Ok(ChunkChangesChunkDecoder {
            length,
            position: 0,
            data: &self.buffer[offset ..],
            _component: PhantomData,
        })
    }

    pub fn encode_chunks(
        chunk_amount: usize,
        buffer: &'a mut Vec<u8>,
    ) -> ChunkChangesChunkEncoder<'a, T> {
        buffer.clear();

        let chunk_amount: u64 = chunk_amount.try_into().unwrap();
        pack::encode_write(&chunk_amount, buffer);

        ChunkChangesChunkEncoder {
            buffer,
            _component: PhantomData,
        }
    }
}

pub struct ChunkChangesBlockDecoder<'origin, 'data, T> {
    origin: &'origin mut ChunkChangesChunkDecoder<'data, T>,
    chunk: Chunk,
    length: usize,
    position: usize,
    _component: PhantomData<T>,
}

impl<T> ChunkChangesBlockDecoder<'_, '_, T> {
    pub fn chunk(&self) -> Chunk {
        self.chunk
    }
}

impl<T> ChunkChangesBlockDecoder<'_, '_, T>
where
    T: DeserializeOwned,
{
    pub fn decode_block(&mut self) -> Option<Result<(Block, T), UnpackError>> {
        if self.position >= self.length {
            return None;
        }

        let (value, offset) = match pack::decode_from_slice::<(Block, T)>(self.origin.data) {
            Some(v) => v,
            None => return Some(Err(UnpackError)),
        };

        self.origin.data = &self.origin.data[offset ..];

        self.position += 1;

        Some(Ok(value))
    }
}

pub struct ChunkChangesChunkDecoder<'a, T> {
    length: usize,
    position: usize,
    data: &'a [u8],
    _component: PhantomData<T>,
}

impl<'a, T> ChunkChangesChunkDecoder<'a, T> {
    pub fn decode_chunk<'b>(
        &'b mut self,
    ) -> Option<Result<ChunkChangesBlockDecoder<'b, 'a, T>, UnpackError>> {
        if self.position >= self.length {
            return None;
        }

        let (chunk, offset) = match pack::decode_from_slice::<Chunk>(self.data) {
            Some(v) => v,
            None => return Some(Err(UnpackError)),
        };

        self.data = &self.data[offset ..];

        let (length, offset) = match pack::decode_from_slice::<u64>(self.data) {
            Some(v) => v,
            None => return Some(Err(UnpackError)),
        };

        self.data = &self.data[offset ..];

        self.position += 1;

        Some(Ok(ChunkChangesBlockDecoder {
            chunk,
            origin: self,
            length: length.try_into().unwrap(),
            position: 0,
            _component: PhantomData,
        }))
    }
}

pub struct ChunkChangesBlockEncoder<'a, T> {
    buffer: &'a mut Vec<u8>,
    _component: PhantomData<T>,
}

impl<'a, T> ChunkChangesBlockEncoder<'a, T>
where
    T: Serialize,
{
    pub fn add_change(&mut self, block: Block, block_component: T) {
        pack::encode_write(&(block, block_component), self.buffer);
    }

    pub fn finish_chunk(self) -> ChunkChangesChunkEncoder<'a, T> {
        ChunkChangesChunkEncoder {
            buffer: self.buffer,
            _component: PhantomData,
        }
    }
}

pub struct ChunkChangesChunkEncoder<'a, T> {
    buffer: &'a mut Vec<u8>,
    _component: PhantomData<T>,
}

impl<'a, T> ChunkChangesChunkEncoder<'a, T> {
    pub fn start_chunk(
        self,
        chunk: &Chunk,
        block_changes_amount: usize,
    ) -> ChunkChangesBlockEncoder<'a, T> {
        let block_changes_amount: u64 = block_changes_amount.try_into().unwrap();
        pack::encode_write(chunk, self.buffer);
        pack::encode_write(&block_changes_amount, self.buffer);

        ChunkChangesBlockEncoder {
            buffer: self.buffer,
            _component: PhantomData,
        }
    }

    pub fn finish(self) -> ChunkChanges<'a, T> {
        ChunkChanges {
            buffer: self.buffer,
            _component: PhantomData,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ServerState<'a> {
    pub snapshot: ServerSnapshot,
    // last client's snapshot received by the server
    pub last_client_snapshot: ClientSnapshot,
    #[serde(borrow)]
    pub updates: UpdatesPacked<'a>,
    #[serde(borrow)]
    pub dispatches: DispatchesPacked<'a>,
}

#[derive(Serialize, Deserialize)]
pub struct ChunkDataDelta<'a> {
    #[serde(borrow)]
    pub block_class: ChunkChanges<'a, BlockClass>,
    #[serde(borrow)]
    pub block_environment: ChunkChanges<'a, BlockEnvironment>,
    #[serde(borrow)]
    pub block_metadata: ChunkChanges<'a, BlockMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ClientAcceptKind {
    State = 0,
    ChunkData = 1,
    ChunkDataDelta = 2,
}

impl ClientAcceptKind {
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::State),
            1 => Some(Self::ChunkData),
            2 => Some(Self::ChunkDataDelta),
            _ => None,
        }
    }
}

/// Wire layout: [tag: u8 = ClientAcceptKind] [payload: pack_compressed_append].
pub struct ClientAcceptMessage {
    bytes: Vec<u8>,
}

impl ClientAcceptMessage {
    fn pack_with<T: Serialize>(packer: &mut Packer, kind: ClientAcceptKind, value: &T) -> Self {
        let mut bytes = Vec::new();
        bytes.push(kind as u8);
        packer.pack_compressed_append(value, &mut bytes);
        Self { bytes }
    }

    pub fn pack_state(packer: &mut Packer, value: &ServerState<'_>) -> Self {
        Self::pack_with(packer, ClientAcceptKind::State, value)
    }

    pub fn pack_chunk_data(packer: &mut Packer, value: &[Arc<[u8]>]) -> Self {
        Self::pack_with(packer, ClientAcceptKind::ChunkData, &value)
    }

    pub fn pack_chunk_data_delta(packer: &mut Packer, value: &ChunkDataDelta<'_>) -> Self {
        Self::pack_with(packer, ClientAcceptKind::ChunkDataDelta, value)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, UnpackError> {
        let tag = *bytes.first().ok_or(UnpackError)?;
        ClientAcceptKind::from_byte(tag).ok_or(UnpackError)?;
        Ok(Self { bytes })
    }

    pub fn from_slice(bytes: &[u8]) -> Result<ClientAcceptMessageRef<'_>, UnpackError> {
        let tag = *bytes.first().ok_or(UnpackError)?;
        ClientAcceptKind::from_byte(tag).ok_or(UnpackError)?;
        Ok(ClientAcceptMessageRef { bytes })
    }

    pub fn kind(&self) -> ClientAcceptKind {
        ClientAcceptKind::from_byte(self.bytes[0]).expect("tag byte validated on construction")
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn unpack_state<'a>(
        &'a self,
        packer: &'a mut Packer,
    ) -> Result<ServerState<'a>, UnpackError> {
        if self.kind() != ClientAcceptKind::State {
            return Err(UnpackError);
        }
        packer.unpack_compressed::<ServerState<'a>>(&self.bytes[1 ..])
    }

    pub fn unpack_chunk_data<'a>(
        &'a self,
        packer: &'a mut Packer,
    ) -> Result<Vec<&'a [u8]>, UnpackError> {
        if self.kind() != ClientAcceptKind::ChunkData {
            return Err(UnpackError);
        }
        packer.unpack_compressed::<Vec<&'a [u8]>>(&self.bytes[1 ..])
    }

    pub fn unpack_chunk_data_delta<'a>(
        &'a self,
        packer: &'a mut Packer,
    ) -> Result<ChunkDataDelta<'a>, UnpackError> {
        if self.kind() != ClientAcceptKind::ChunkDataDelta {
            return Err(UnpackError);
        }
        packer.unpack_compressed::<ChunkDataDelta<'a>>(&self.bytes[1 ..])
    }
}

pub struct ClientAcceptMessageRef<'a> {
    bytes: &'a [u8],
}

impl<'a> ClientAcceptMessageRef<'a> {
    pub fn kind(&self) -> ClientAcceptKind {
        ClientAcceptKind::from_byte(self.bytes[0]).expect("tag byte validated on construction")
    }

    pub fn unpack_state<'b>(
        &'b self,
        packer: &'b mut Packer,
    ) -> Result<ServerState<'b>, UnpackError> {
        if self.kind() != ClientAcceptKind::State {
            return Err(UnpackError);
        }
        packer.unpack_compressed::<ServerState<'b>>(&self.bytes[1 ..])
    }

    pub fn unpack_chunk_data<'b>(
        &'b self,
        packer: &'b mut Packer,
    ) -> Result<Vec<&'b [u8]>, UnpackError> {
        if self.kind() != ClientAcceptKind::ChunkData {
            return Err(UnpackError);
        }
        packer.unpack_compressed::<Vec<&'b [u8]>>(&self.bytes[1 ..])
    }

    pub fn unpack_chunk_data_delta<'b>(
        &'b self,
        packer: &'b mut Packer,
    ) -> Result<ChunkDataDelta<'b>, UnpackError> {
        if self.kind() != ClientAcceptKind::ChunkDataDelta {
            return Err(UnpackError);
        }
        packer.unpack_compressed::<ChunkDataDelta<'b>>(&self.bytes[1 ..])
    }
}
