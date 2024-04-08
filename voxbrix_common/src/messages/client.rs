use crate::{
    entity::{
        actor::Actor,
        block::Block,
        block_class::BlockClass,
        chunk::Chunk,
        snapshot::Snapshot,
    },
    messages::StatePacked,
    pack::{
        self,
        Pack,
        UnpackError,
    },
    ChunkData,
};
use bincode::{
    BorrowDecode,
    Decode,
    Encode,
};

#[derive(Encode, Decode)]
pub struct InitResponse {
    pub public_key: [u8; 33],
    pub key_signature: [u8; 64],
}

impl Pack for InitResponse {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Encode, Decode, Debug)]
pub enum LoginFailure {
    IncorrectCredentials,
    Unknown,
}

#[derive(Encode, Decode, Debug)]
pub enum RegisterFailure {
    UsernameTaken,
    Unknown,
}

#[derive(Encode, Decode)]
pub struct InitData {
    pub actor: Actor,
    // position: Position,
    pub player_chunk_view_radius: i32,
}

impl Pack for InitData {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Encode, Decode)]
pub enum LoginResult {
    Success(InitData),
    Failure(LoginFailure),
}

impl Pack for LoginResult {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Encode, Decode)]
pub enum RegisterResult {
    Success(InitData),
    Failure(RegisterFailure),
}

impl Pack for RegisterResult {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Encode, BorrowDecode)]
pub struct ChunkChanges<'a>(&'a [u8]);

impl<'a> ChunkChanges<'a> {
    pub fn decode_chunks(self) -> Result<ChunkChangesChunkDecoder<'a>, UnpackError> {
        let (length, offset) = pack::decode_from_slice::<u64>(self.0).ok_or(UnpackError)?;

        let length = length.try_into().unwrap();

        Ok(ChunkChangesChunkDecoder {
            length,
            position: 0,
            data: &self.0[offset ..],
        })
    }

    pub fn encode_chunks(
        chunk_amount: usize,
        buffer: &'a mut Vec<u8>,
    ) -> ChunkChangesChunkEncoder<'a> {
        buffer.clear();

        let chunk_amount: u64 = chunk_amount.try_into().unwrap();
        pack::encode_write(&chunk_amount, buffer);

        ChunkChangesChunkEncoder(buffer)
    }
}

pub struct ChunkChangesBlockDecoder<'a> {
    origin: ChunkChangesChunkDecoder<'a>,
    chunk: Chunk,
    length: usize,
    position: usize,
}

impl<'a> ChunkChangesBlockDecoder<'a> {
    pub fn chunk(&self) -> Chunk {
        self.chunk
    }

    pub fn decode_block(&mut self) -> Option<Result<(Block, BlockClass), UnpackError>> {
        if self.position >= self.length {
            return None;
        }

        let (value, offset) = match pack::decode_from_slice::<(Block, BlockClass)>(self.origin.data)
        {
            Some(v) => v,
            None => return Some(Err(UnpackError)),
        };

        self.origin.data = &self.origin.data[offset ..];

        self.position += 1;

        Some(Ok(value))
    }

    pub fn into_chunk_decoder(self) -> ChunkChangesChunkDecoder<'a> {
        self.origin
    }
}

pub struct ChunkChangesChunkDecoder<'a> {
    length: usize,
    position: usize,
    data: &'a [u8],
}

impl<'a> ChunkChangesChunkDecoder<'a> {
    pub fn decode_chunk(mut self) -> Option<Result<ChunkChangesBlockDecoder<'a>, UnpackError>> {
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
        }))
    }
}

pub struct ChunkChangesBlockEncoder<'a>(&'a mut Vec<u8>);

impl<'a> ChunkChangesBlockEncoder<'a> {
    pub fn add_change(&mut self, block: Block, block_class: BlockClass) {
        pack::encode_write(&(block, block_class), self.0);
    }

    pub fn finish_chunk(self) -> ChunkChangesChunkEncoder<'a> {
        ChunkChangesChunkEncoder(self.0)
    }
}

pub struct ChunkChangesChunkEncoder<'a>(&'a mut Vec<u8>);

impl<'a> ChunkChangesChunkEncoder<'a> {
    pub fn start_chunk(
        self,
        chunk: &Chunk,
        block_changes_amount: usize,
    ) -> ChunkChangesBlockEncoder<'a> {
        let block_changes_amount: u64 = block_changes_amount.try_into().unwrap();
        pack::encode_write(chunk, self.0);
        pack::encode_write(&block_changes_amount, self.0);

        ChunkChangesBlockEncoder(self.0)
    }

    pub fn finish(self) -> ChunkChanges<'a> {
        ChunkChanges(self.0)
    }
}

#[derive(Encode, BorrowDecode)]
pub enum ClientAccept<'a> {
    State {
        snapshot: Snapshot,
        // last client's snapshot received by the server
        last_client_snapshot: Snapshot,
        state: StatePacked<'a>,
    },
    ChunkData(ChunkData),
    ChunkChanges(ChunkChanges<'a>),
}

impl Pack for ClientAccept<'_> {
    const DEFAULT_COMPRESSED: bool = true;
}
