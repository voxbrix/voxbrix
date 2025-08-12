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
    Dispatch,
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
pub extern "C" fn run() {
    api::handle_panic(SCRIPT_NAME);

    let input = api::read_action_input::<PlaceBlock>().expect("incorrect input");

    // FIXME use label map to get correct dispatch.
    if let Some(actor) = input.actor {
        api::broadcast_dispatch_local(Dispatch(0), actor, ());
    }

    let Some(target) = api::get_target_block(GetTargetBlockRequest {
        chunk: input.data.chunk,
        offset: input.data.offset,
        direction: input.data.direction,
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
            block_class: input.data.block_class,
        });
    }
}
