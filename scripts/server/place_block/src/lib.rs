use paste::paste;
use std::panic::{self, PanicInfo};
use bincode::config::Configuration;
use bincode::Decode;

static CODE_CONFIG: Configuration = bincode::config::standard();

extern "C" {
    fn handle_panic(msg_ptr: *const u8, msg_len: u32);
    fn log_message(msg_ptr: *const u8, msg_len: u32);
    /*
    fn get_block_class_by_label(ptr: *const u8, len: u32) -> u64;
    fn get_class_of_block(chunk_x: i32, chunk_y: i32, chunk_z: i32, block: u16) -> u64;
    fn set_class_of_block(chunk_x: i32, chunk_y: i32, chunk_z: i32, block: u16, class: u64);
    fn get_last_transparent_block(
        input_ptr: *mut u8,
        input_len: u32,
    );
    fn get_script_by_label(ptr: *const u8, len: u32) -> u64;
    /// The script will run after the current returns
    fn run_script(script: u64, input_ptr: *const u8, input_len: u32);
    */
}

macro_rules! block_class {
    ($name:ident) => {
        unsafe {
            paste! {
                static [<$name:upper _NAME>]: &'static str = stringify!($name);
                static mut [<$name:upper>]: Option<u64> = None;
                if [<$name:upper>].is_none() {
                    [<$name:upper>] = Some(get_block_class_by_label(
                        [<$name:upper _NAME>].as_ptr(),
                        [<$name:upper _NAME>].len() as u32,
                    ))
                }
                [<$name:upper>].unwrap()
            }
        }
    };
}

static SCRIPT_NAME: &'static str = "place_block";

#[no_mangle]
pub extern "C" fn start() {
    panic::set_hook(Box::new(|panic_info| {
        let msg = format!("script \"{}\": {}", SCRIPT_NAME, panic_info);
        unsafe {
            handle_panic(msg.as_ptr(), msg.len() as u32);
        }
    }));
}

static mut SHARED_BUFFER: Vec<u8> = Vec::new();

fn prepare_shared_buffer(len: usize) -> &'static mut [u8] {
    unsafe {
        if SHARED_BUFFER.len() < len {
            let mut new_buffer = Vec::with_capacity(len);
            new_buffer.set_len(len);
            SHARED_BUFFER = new_buffer;
        }

        SHARED_BUFFER.as_mut()
    }
}

// TODO: must not accept 0 length
#[no_mangle]
pub extern "C" fn write_buffer(len: u32) -> *mut u8 {
    prepare_shared_buffer(len as usize).as_mut_ptr()
}

fn read_buffer<T>(len: usize) -> T
where
    T: Decode,
{
    let input_slice = unsafe {
        &SHARED_BUFFER[.. len as usize]
    };

    bincode::decode_from_slice(input_slice, CODE_CONFIG).unwrap().0
}

/*
#[derive(Serialize, Deserialize)]
pub struct Action {
    chunk: [i32; 3],
    position: [f32; 3],
    orientation: [f32; 3],
}*/

#[no_mangle]
pub extern "C" fn run(input_len: u32) {
    let (player, action) = read_buffer::<(u64, String)>(input_len as usize);

    let full_message = format!("got action: {}", action);

    unsafe {
        log_message(full_message.as_ptr(), full_message.len() as u32);
    }
}
