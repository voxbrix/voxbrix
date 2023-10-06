use paste::paste;

extern "C" {
    fn get_blocks_in_chunk_edge() -> u32;
    fn get_block_class(ptr: *const u8, len: u32) -> u64;
    fn push_block(block_class: u64);
}

macro_rules! block_class {
    ($name:ident) => {
        unsafe {
            paste! {
                static [<$name:upper _NAME>]: &'static str = stringify!($name);
                static mut [<$name:upper>]: Option<u64> = None;
                if [<$name:upper>].is_none() {
                    [<$name:upper>] = Some(get_block_class(
                        [<$name:upper _NAME>].as_ptr(),
                        [<$name:upper _NAME>].len() as u32,
                    ))
                }
                [<$name:upper>].unwrap()
            }
        }
    };
}

#[no_mangle]
pub extern "C" fn generate_chunk(
    _dimension_index: u32,
    _chunk_x: i32,
    _chunk_y: i32,
    chunk_z: i32,
) {
    let blocks_in_chunk_edge = unsafe {
        static mut BICE: Option<usize> = None;
        if BICE.is_none() {
            BICE = Some(get_blocks_in_chunk_edge() as usize)
        }
        BICE.unwrap()
    };

    let air = block_class!(air);
    let grass = block_class!(grass);
    let stone = block_class!(stone);

    let push_block = |block_class: u64| unsafe {
        push_block(block_class);
    };

    for z in 0..blocks_in_chunk_edge {
        for _y in 0..blocks_in_chunk_edge {
            for _x in 0..blocks_in_chunk_edge {
                if chunk_z >= 0 {
                    push_block(air);
                } else if z == blocks_in_chunk_edge - 1 {
                    push_block(grass);
                } else {
                    push_block(stone);
                }
            }
        }
    }
}
