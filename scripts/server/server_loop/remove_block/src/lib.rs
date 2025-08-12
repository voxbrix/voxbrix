use server_loop_api::{
    self as api,
    serde::{
        self,
        Deserialize,
        Serialize,
    },
    BlockClass,
    Chunk,
    Dispatch,
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
pub extern "C" fn run() {
    api::handle_panic(SCRIPT_NAME);

    let input = api::read_action_input::<RemoveBlock>().expect("incorrect input");

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

    let air = api::block_class!(air);

    api::set_class_of_block(SetClassOfBlockRequest {
        chunk: target.chunk,
        block: target.block,
        block_class: air,
    });
}
