use crate::common::*;
use bincode::{
    config::Configuration,
    Decode,
    Encode,
};

static CODE_CONFIG: Configuration = bincode::config::standard();

static mut SHARED_BUFFER: Vec<u8> = Vec::new();

mod export {
    extern "C" {
        pub fn get_blocks_in_chunk_edge() -> u32;
        pub fn get_target_block(ptr: *const u8, len: u32) -> u32;
        pub fn set_class_of_block(ptr: *const u8, len: u32);
        pub fn get_block_class_by_label(ptr: *const u8, len: u32) -> u32;
        // fn get_class_of_block(chunk_x: i32, chunk_y: i32, chunk_z: i32, block: u16) -> u64;
        // fn set_class_of_block(chunk_x: i32, chunk_y: i32, chunk_z: i32, block: u16, class: u64);
        // fn get_last_transparent_block(
        // input_ptr: *mut u8,
        // input_len: u32,
        // );
        // fn get_script_by_label(ptr: *const u8, len: u32) -> u64;
        // The script will run after the current returns
        // fn run_script(script: u64, input_ptr: *const u8, input_len: u32);
    }
}

macro_rules! wrap_func {
    ($name:ident, $input_type:ty) => {
        pub fn $name(input: $input_type) {
            let req = write_buffer(input);

            unsafe { export::$name(req.as_ptr(), req.len() as u32) };
        }
    };
    ($name:ident, $input_type:ty, $output_type:ty) => {
        pub fn $name(input: $input_type) -> $output_type {
            let req = write_buffer(input);

            let resp_len = unsafe { export::$name(req.as_ptr(), req.len() as u32) };

            read_buffer::<$output_type>(resp_len as usize)
        }
    };
}

fn prepare_shared_buffer(len: usize) -> &'static mut [u8] {
    unsafe {
        if SHARED_BUFFER.capacity() < len {
            SHARED_BUFFER.reserve(len - SHARED_BUFFER.capacity());
        }

        SHARED_BUFFER.set_len(len);
        SHARED_BUFFER.as_mut()
    }
}

// TODO: must not accept 0 length
#[no_mangle]
pub extern "C" fn get_buffer(len: u32) -> *mut u8 {
    prepare_shared_buffer(len as usize).as_mut_ptr()
}

pub fn read_buffer<T>(len: usize) -> T
where
    T: Decode,
{
    let input_slice = unsafe { &SHARED_BUFFER[.. len as usize] };

    bincode::decode_from_slice(input_slice, CODE_CONFIG)
        .unwrap()
        .0
}

pub fn write_buffer<T>(value: T) -> &'static [u8]
where
    T: Encode,
{
    unsafe {
        SHARED_BUFFER.clear();

        bincode::encode_into_std_write(value, &mut SHARED_BUFFER, CODE_CONFIG).unwrap();

        SHARED_BUFFER.as_slice()
    }
}

static mut BLOCKS_IN_CHUNK_EDGE: usize = 0;
static mut BLOCKS_IN_CHUNK_LAYER: usize = 0;
static mut BLOCKS_IN_CHUNK: usize = 0;

pub fn blocks_in_chunk_edge() -> usize {
    unsafe {
        if BLOCKS_IN_CHUNK_EDGE == 0 {
            BLOCKS_IN_CHUNK_EDGE = export::get_blocks_in_chunk_edge()
                .try_into()
                .expect("BLOCKS_IN_CHUNK_EDGE provided is more than u16::MAX");
        }

        BLOCKS_IN_CHUNK_EDGE
    }
}

pub fn blocks_in_chunk_layer() -> usize {
    unsafe {
        if BLOCKS_IN_CHUNK_LAYER == 0 {
            BLOCKS_IN_CHUNK_LAYER = blocks_in_chunk_edge().pow(2);
        }

        BLOCKS_IN_CHUNK_LAYER
    }
}

pub fn blocks_in_chunk() -> usize {
    unsafe {
        if BLOCKS_IN_CHUNK_LAYER == 0 {
            BLOCKS_IN_CHUNK_LAYER = blocks_in_chunk_edge().pow(2);
        }

        BLOCKS_IN_CHUNK_LAYER
    }
}

impl Block {
    pub fn into_coords(self) -> [usize; 3] {
        let z = self.as_usize() / blocks_in_chunk_layer();
        let x_y = self.as_usize() % blocks_in_chunk_layer();
        let y = x_y / blocks_in_chunk_edge();
        let x = x_y % blocks_in_chunk_edge();

        [x, y, z]
    }

    pub fn from_coords([x, y, z]: [usize; 3]) -> Self {
        Self::from_usize(z * blocks_in_chunk_layer() + y * blocks_in_chunk_edge() + x).unwrap()
    }

    pub fn from_chunk_offset(chunk: Chunk, offset: [i32; 3]) -> Option<(Chunk, Block)> {
        let blocks_in_chunk_edge_i32 = blocks_in_chunk_edge() as i32;

        let chunks_blocks = offset.map(|offset| {
            let mut chunk_offset = offset / blocks_in_chunk_edge_i32;
            let mut block = offset % blocks_in_chunk_edge_i32;

            if block < 0 {
                chunk_offset -= 1;
                block += blocks_in_chunk_edge_i32;
            }

            (chunk_offset, block)
        });

        let actual_chunk = Chunk {
            position: [
                chunks_blocks[0].0.checked_add(chunk.position[0])?,
                chunks_blocks[1].0.checked_add(chunk.position[1])?,
                chunks_blocks[2].0.checked_add(chunk.position[2])?,
            ],
            dimension: chunk.dimension,
        };

        let block = Self::from_coords([chunks_blocks[0].1, chunks_blocks[1].1, chunks_blocks[2].1]);

        Some((actual_chunk, block))
    }
}

wrap_func!(
    get_target_block,
    GetTargetBlockRequest,
    Option<GetTargetBlockResponse>
);

wrap_func!(get_block_class_by_label, &str, Option<BlockClass>);

#[macro_export]
macro_rules! block_class {
    ($name:ident) => {
        unsafe {
            paste! {
                static [<$name:upper _NAME>]: &'static str = stringify!($name);
                static mut [<$name:upper>]: Option<BlockClass> = None;
                if [<$name:upper>].is_none() {
                    [<$name:upper>] = Some(::server_loop_api::get_block_class_by_label(
                        [<$name:upper _NAME>]
                    ).expect("block class not found"))
                }
                [<$name:upper>].unwrap()
            }
        }
    };
}

wrap_func!(set_class_of_block, SetClassOfBlockRequest);
