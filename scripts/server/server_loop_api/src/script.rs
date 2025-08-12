use crate::common::*;
pub use paste::paste;
use postcard::ser_flavors::Flavor;
use serde::{
    de::DeserializeOwned,
    Serialize,
};
use std::{
    cell::RefCell,
    io::Write,
    panic,
};

thread_local! {
    static SHARED_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

mod import {
    extern "C" {
        pub fn handle_panic(ptr: *const u8, len: u32);
        pub fn get_blocks_in_chunk_edge() -> u32;

        // The ones below use postcard to serialize input/output from/into shared buffer:
        pub fn get_target_block(ptr: *const u8, len: u32);
        pub fn set_class_of_block(ptr: *const u8, len: u32);
        pub fn get_block_class_by_label(ptr: *const u8, len: u32);
        pub fn broadcast_dispatch_local(ptr: *const u8, len: u32);
    }
}

pub fn handle_panic(script_name: &'static str) {
    panic::set_hook(Box::new(move |panic_info| {
        SHARED_BUFFER.with_borrow_mut(|shared_buffer| {
            shared_buffer.clear();

            let _ = write!(shared_buffer, "script \"{}\": {}", script_name, panic_info);

            unsafe {
                import::handle_panic(shared_buffer.as_ptr(), shared_buffer.len() as u32);
            }
        });
    }));
}

macro_rules! wrap_func {
    ($name:ident, $input_type:ty) => {
        pub fn $name(input: $input_type) {
            let (req_ptr, req_len) = write_buffer(input);

            unsafe { import::$name(req_ptr, req_len as u32) };
        }
    };
    ($name:ident, $input_type:ty, $output_type:ty) => {
        pub fn $name(input: $input_type) -> $output_type {
            let (req_ptr, req_len) = write_buffer(input);

            unsafe { import::$name(req_ptr, req_len as u32) };

            read_buffer::<$output_type>().expect("incorrect host response")
        }
    };
}

/// Get pointer to the shared buffer of given length. Will reallocate the buffer if required.
/// Only use this for writing into the buffer. Existing data inside will be garbage.
// TODO: must not accept 0 length
#[no_mangle]
pub extern "C" fn get_buffer(len: u32) -> *mut u8 {
    let len = len as usize;

    SHARED_BUFFER.with_borrow_mut(|shared_buffer| {
        if shared_buffer.capacity() < len {
            shared_buffer.reserve(len - shared_buffer.capacity());
        }

        // SAFETY: we set capacity on the previous step.
        unsafe {
            shared_buffer.set_len(len);
        }
        shared_buffer.as_mut_ptr()
    })
}

/// Deserialize value from the shared buffer.
pub fn read_buffer<T>() -> Option<T>
where
    T: DeserializeOwned,
{
    SHARED_BUFFER.with_borrow(|shared_buffer| postcard::from_bytes(shared_buffer.as_slice()).ok())
}

pub struct ActionInputParsed<T> {
    pub action: Action,
    pub actor: Option<Actor>,
    pub data: T,
}

/// Deserialize action input from the shared buffer.
pub fn read_action_input<T>() -> Option<ActionInputParsed<T>>
where
    T: DeserializeOwned,
{
    SHARED_BUFFER.with_borrow(|shared_buffer| {
        let input = postcard::from_bytes::<ActionInput>(shared_buffer.as_slice()).ok()?;

        let data = postcard::from_bytes::<T>(input.data).ok()?;

        Some(ActionInputParsed {
            action: input.action,
            actor: input.actor,
            data,
        })
    })
}

/// Broadcast dispatch to local (within chunk view) players.
pub fn broadcast_dispatch_local<T>(dispatch: Dispatch, actor: Actor, data: T)
where
    T: Serialize,
{
    thread_local! {
        static BROADCAST_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    }

    BROADCAST_BUFFER.with_borrow_mut(|broadcast_buffer| {
        broadcast_buffer.clear();

        postcard::serialize_with_flavor(
            &data,
            Writer {
                written: 0,
                writer: &mut *broadcast_buffer,
            },
        )
        .unwrap();

        let (input_slice_ptr, input_slice_len) = write_buffer(BroadcastDispatchLocalRequest {
            dispatch,
            actor,
            data: broadcast_buffer.as_slice(),
        });

        unsafe {
            import::broadcast_dispatch_local(input_slice_ptr, input_slice_len.try_into().unwrap());
        }
    })
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

/// Serialize the value into the shared buffer.
/// WARNING: this will overwrite content of the shared buffer.
pub fn write_buffer<T>(value: T) -> (*const u8, usize)
where
    T: Serialize,
{
    SHARED_BUFFER.with_borrow_mut(|shared_buffer| {
        shared_buffer.clear();

        postcard::serialize_with_flavor(
            &value,
            Writer {
                written: 0,
                writer: &mut *shared_buffer,
            },
        )
        .unwrap();

        let slice = shared_buffer.as_slice();

        (slice.as_ptr(), slice.len())
    })
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
