use bincode::{
    Decode,
    Encode,
};

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct Dimension {
    pub index: u32,
}

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct Chunk {
    pub position: [i32; 3],
    pub dimension: Dimension,
}

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct Block(pub u16);

#[derive(Encode, Decode, Debug)]
pub struct GetTargetBlockRequest {
    pub chunk: Chunk,
    pub offset: [f32; 3],
    pub direction: [f32; 3],
}

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct BlockClass(pub u64);

#[derive(Encode, Decode, Clone, Copy, Debug)]
pub struct Actor(pub u64);

#[derive(Encode, Decode, Debug)]
pub struct GetTargetBlockResponse {
    pub chunk: Chunk,
    pub block: Block,
    pub side: u8,
}

#[derive(Encode, Decode, Debug)]
pub struct SetClassOfBlockRequest {
    pub chunk: Chunk,
    pub block: Block,
    pub block_class: BlockClass,
}
