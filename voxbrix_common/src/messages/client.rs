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
        UnpackError,
    },
    ChunkData,
};
use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};
use std::marker::PhantomData;

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
    pub player_chunk_view_radius: i32,
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
pub enum ClientAccept<'a> {
    State(ServerState<'a>),
    ChunkData(ChunkData),
    ChunkChanges {
        #[serde(borrow)]
        block_class: ChunkChanges<'a, BlockClass>,
        #[serde(borrow)]
        block_environment: ChunkChanges<'a, BlockEnvironment>,
        #[serde(borrow)]
        block_metadata: ChunkChanges<'a, BlockMetadata>,
    },
}

impl Pack for ClientAccept<'_> {
    const DEFAULT_COMPRESSED: bool = true;
}
