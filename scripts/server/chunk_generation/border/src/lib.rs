use chunk_generation_api::{
    self as api,
    hash::Hasher64,
    noise_2d,
    pack_block_data,
};

#[no_mangle]
pub extern "C" fn generate_chunk(seed: u64, phase: u64, chunk_x: i32, chunk_y: i32, chunk_z: i32) {
    let blocks_in_chunk_edge = api::get_blocks_in_chunk_edge();

    let empty = api::block_class!(empty);
    let grass = api::block_class!(grass);

    let air = api::block_environment!(air);
    let water = api::block_environment!(water);

    let mut hasher = Hasher64::new(seed);
    hasher.write(&phase.to_le_bytes());
    hasher.write(&(chunk_z / 8).to_le_bytes());
    let seed = hasher.finish();

    for block_z in 0 .. blocks_in_chunk_edge {
        for block_y in 0 .. blocks_in_chunk_edge {
            for block_x in 0 .. blocks_in_chunk_edge {
                let block_value = noise_2d(
                    seed,
                    64,
                    blocks_in_chunk_edge,
                    [chunk_x, chunk_y],
                    [block_x, block_y],
                );

                let empty_block = if chunk_z < 1 { water } else { air };

                let ground_block_z = blocks_in_chunk_edge - 1;

                let data = if chunk_z % 32 == 0 && (0 ..= ground_block_z).contains(&block_z) {
                    let width_coef = block_z as f64 / ground_block_z as f64;

                    let block_value = (1.0 - block_value.abs()) * (0.8 + 0.2 * width_coef);

                    if block_value > 0.95 {
                        pack_block_data(grass, empty_block, 0)
                    } else {
                        pack_block_data(empty, empty_block, 0)
                    }
                } else {
                    pack_block_data(empty, empty_block, 0)
                };

                api::push_block(data);
            }
        }
    }
}
