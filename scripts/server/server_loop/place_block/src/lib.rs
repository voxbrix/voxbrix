use std::panic;
use bincode::{Encode, Decode};
use server_loop_api::{
    self as api,
    GetTargetBlockRequest,
    SetClassOfBlockRequest,
    Actor,
    Block,
    BlockClass,
    Chunk,
};

extern "C" {
    fn handle_panic(ptr: *const u8, len: u32);
    //fn log_message(ptr: *const u8, len: u32);
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

#[derive(Encode, Decode)]
pub struct PlaceBlock {
    chunk: Chunk,
    offset: [f32; 3],
    direction: [f32; 3],
    block_class: BlockClass,
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
    ) = api::read_buffer::<(Option<Actor>, u64, PlaceBlock)>(input_len as usize);

    let Some(target) = api::get_target_block(GetTargetBlockRequest {
        chunk: action.chunk,
        offset: action.offset,
        direction: action.direction,
    }) else {
        return;
    };

    let axis = (target.side / 2) as usize;
    let direction = match target.side % 2 {
        0 => -1,
        1 => 1,
        _ => panic!("incorrect side index"),
    };
    let mut block_offset = target.block.into_coords().map(|u| u as i32);
    block_offset[axis] += direction;
    if let Some((chunk, block)) =
        Block::from_chunk_offset(target.chunk, block_offset)
    {
        api::set_class_of_block(SetClassOfBlockRequest {
            chunk,
            block,
            block_class: action.block_class,
        });
    }
}
