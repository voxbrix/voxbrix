use std::panic;
use bincode::{Encode, Decode};
use server_loop_api::{
    self as api,
    GetTargetBlockRequest,
    SetClassOfBlockRequest,
    Actor,
    BlockClass,
    Chunk,
};
use paste::paste;

extern "C" {
    fn handle_panic(ptr: *const u8, len: u32);
    //fn log_message(ptr: *const u8, len: u32);
}

static SCRIPT_NAME: &'static str = "remove_block";

#[no_mangle]
pub extern "C" fn start() {
    panic::set_hook(Box::new(|panic_info| {
        let msg = format!("script \"{}\": {}", SCRIPT_NAME, panic_info);
        unsafe {
            handle_panic(msg.as_ptr(), msg.len() as u32);
        }
    }));
}

#[derive(Encode, Decode)]
pub struct RemoveBlock {
    chunk: Chunk,
    offset: [f32; 3],
    direction: [f32; 3],
}

#[no_mangle]
pub extern "C" fn run(input_len: u32) {
    panic::set_hook(Box::new(|panic_info| {
        let msg = format!("script \"{}\": {}", SCRIPT_NAME, panic_info);
        unsafe {
            handle_panic(msg.as_ptr(), msg.len() as u32);
        }
    }));
    // Action is prefixed with serialized length as it is represented by byte array on the host.
    let (
        _actor_opt,
        _action_len_prefix,
        action,
    ) = api::read_buffer::<(Option<Actor>, u64, RemoveBlock)>(input_len as usize);

    let Some(target) = api::get_target_block(GetTargetBlockRequest {
        chunk: action.chunk,
        offset: action.offset,
        direction: action.direction,
    }) else {
        return;
    };

    let air = api::block_class!(air);

    api::set_class_of_block(SetClassOfBlockRequest {
        chunk: target.chunk,
        block: target.block,
        block_class: air,
    });
}
