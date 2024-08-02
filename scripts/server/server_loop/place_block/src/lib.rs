use server_loop_api::{
    self as api,
    serde::{
        self,
        Deserialize,
        Serialize,
    },
    Block,
    BlockClass,
    Chunk,
    GetTargetBlockRequest,
    SetClassOfBlockRequest,
};

static SCRIPT_NAME: &'static str = "place_block";

#[derive(Serialize, Deserialize)]
#[serde(crate = "self::serde")]
pub struct PlaceBlock {
    chunk: Chunk,
    offset: [f32; 3],
    direction: [f32; 3],
    block_class: BlockClass,
}

#[no_mangle]
pub extern "C" fn run(input_len: u32) {
    api::handle_panic(SCRIPT_NAME);

    // Action is prefixed with serialized length as it is represented by byte array on the host.
    let Some((_actor_opt, action)) = api::read_action_input::<PlaceBlock>(input_len as usize)
    else {
        return;
    };

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
    if let Some((chunk, block)) = Block::from_chunk_offset(target.chunk, block_offset) {
        api::set_class_of_block(SetClassOfBlockRequest {
            chunk,
            block,
            block_class: action.block_class,
        });
    }
}
