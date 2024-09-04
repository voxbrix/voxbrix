use crate::common::*;
pub use paste::paste;
use postcard::ser_flavors::Flavor;
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::{
    io::Write,
    panic,
    ptr,
};

static mut SHARED_BUFFER: Vec<u8> = Vec::new();

mod import {
    extern "C" {
        pub fn handle_panic(ptr: *const u8, len: u32);
        pub fn get_blocks_in_chunk_edge() -> u32;
        pub fn get_target_block(ptr: *const u8, len: u32) -> u32;
        pub fn set_class_of_block(ptr: *const u8, len: u32);
        pub fn get_block_class_by_label(ptr: *const u8, len: u32) -> u32;
        pub fn broadcast_action_local(ptr: *const u8, len: u32);
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

pub fn handle_panic(script_name: &'static str) {
    panic::set_hook(Box::new(move |panic_info| {
        let msg = format!("script \"{}\": {}", script_name, panic_info);
        unsafe {
            import::handle_panic(msg.as_ptr(), msg.len() as u32);
        }
    }));
}

macro_rules! wrap_func {
    ($name:ident, $input_type:ty) => {
        pub fn $name(input: $input_type) {
            let req = write_buffer(input);

            unsafe { import::$name(req.as_ptr(), req.len() as u32) };
        }
    };
    ($name:ident, $input_type:ty, $output_type:ty) => {
        pub fn $name(input: $input_type) -> $output_type {
            let req = write_buffer(input);

            let resp_len = unsafe { import::$name(req.as_ptr(), req.len() as u32) };

            read_buffer::<$output_type>(resp_len as usize).expect("incorrect host response")
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

pub fn read_buffer<T>(len: usize) -> Option<T>
where
    T: DeserializeOwned,
{
    let input_slice = unsafe { &SHARED_BUFFER[.. len as usize] };

    postcard::from_bytes(input_slice).ok()
}

pub struct ActionInputParsed<T> {
    pub action: Action,
    pub actor: Option<Actor>,
    pub data: T,
}

pub fn read_action_input<T>(len: usize) -> Option<ActionInputParsed<T>>
where
    T: DeserializeOwned,
{
    let input_slice = unsafe { &SHARED_BUFFER[.. len as usize] };

    let input = postcard::from_bytes::<ActionInput>(input_slice).ok()?;

    let data = postcard::from_bytes::<T>(input.data).ok()?;

    Some(ActionInputParsed {
        action: input.action,
        actor: input.actor,
        data,
    })
}

// TODO instead of None optionally have a possibility to pass a position.
pub fn broadcast_action<T>(action: Action, actor: Option<Actor>, data: T)
where
    T: Serialize,
{
    static mut BROADCAST_BUFFER: Vec<u8> = Vec::new();

    unsafe {
        BROADCAST_BUFFER.clear();

        postcard::serialize_with_flavor(
            &data,
            Writer {
                written: 0,
                writer: &mut *ptr::addr_of_mut!(BROADCAST_BUFFER),
            },
        )
        .unwrap();

        let input_slice = write_buffer(ActionInput {
            action,
            actor,
            data: BROADCAST_BUFFER.as_slice(),
        });

        import::broadcast_action_local(input_slice.as_ptr(), input_slice.len().try_into().unwrap());
    }
}

struct Writer<W> {
    written: usize,
    writer: W,
}

impl<W> Flavor for Writer<W>
where
    W: Write,
{
    type Output = usize;

    fn try_push(&mut self, data: u8) -> postcard::Result<()> {
        self.writer
            .write_all(&[data])
            .map_err(|_| postcard::Error::SerializeBufferFull)?;
        self.written += 1;
        Ok(())
    }

    fn finalize(mut self) -> postcard::Result<Self::Output> {
        self.writer
            .flush()
            .map_err(|_| postcard::Error::SerializeBufferFull)?;
        Ok(self.written)
    }

    fn try_extend(&mut self, data: &[u8]) -> postcard::Result<()> {
        self.writer
            .write_all(data)
            .map_err(|_| postcard::Error::SerializeBufferFull)?;
        self.written += data.len();
        Ok(())
    }
}

pub fn write_buffer<T>(value: T) -> &'static [u8]
where
    T: Serialize,
{
    unsafe {
        SHARED_BUFFER.clear();

        postcard::serialize_with_flavor(
            &value,
            Writer {
                written: 0,
                writer: &mut *ptr::addr_of_mut!(SHARED_BUFFER),
            },
        )
        .unwrap();

        SHARED_BUFFER.as_slice()
    }
}

static mut BLOCKS_IN_CHUNK_EDGE: usize = 0;
static mut BLOCKS_IN_CHUNK_LAYER: usize = 0;
static mut BLOCKS_IN_CHUNK: usize = 0;

pub fn blocks_in_chunk_edge() -> usize {
    unsafe {
        if BLOCKS_IN_CHUNK_EDGE == 0 {
            BLOCKS_IN_CHUNK_EDGE = import::get_blocks_in_chunk_edge()
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
        if BLOCKS_IN_CHUNK == 0 {
            BLOCKS_IN_CHUNK = blocks_in_chunk_edge().pow(3);
        }

        BLOCKS_IN_CHUNK
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

            (chunk_offset, block.try_into().unwrap())
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
            server_loop_api::paste! {
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
