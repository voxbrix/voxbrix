use server_loop_api::{
    self as api,
    serde::{
        self,
        Deserialize,
        Serialize,
    },
    BlockClass,
    Chunk,
    GetTargetBlockRequest,
    SetClassOfBlockRequest,
};

static SCRIPT_NAME: &'static str = "remove_block";

#[derive(Serialize, Deserialize)]
#[serde(crate = "self::serde")]
pub struct RemoveBlock {
    chunk: Chunk,
    offset: [f32; 3],
    direction: [f32; 3],
}

#[no_mangle]
pub extern "C" fn run(input_len: u32) {
    api::handle_panic(SCRIPT_NAME);

    let Some(input) = api::read_action_input::<RemoveBlock>(input_len as usize) else {
        return;
    };

    api::broadcast_action(input.action, input.actor, ());

    let Some(target) = api::get_target_block(GetTargetBlockRequest {
        chunk: input.data.chunk,
        offset: input.data.offset,
        direction: input.data.direction,
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
