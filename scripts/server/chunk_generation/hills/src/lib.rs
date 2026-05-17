use chunk_generation_api::{
    self as api,
    hash::Hasher64,
    noise_2d,
    pack_block_data,
};

/// Maximum amplitude (in blocks) of terrain above/below the water level (global z = 0).
/// Hills can reach roughly +AMPLITUDE blocks, valleys roughly -AMPLITUDE blocks.
const AMPLITUDE: f64 = 64.0;

/// Number of blocks above the surface that get a grass cap.
const GRASS_DEPTH: i64 = 1;

/// Radius (in blocks, in the x/y plane) around the world origin within which the
/// coarsest noise layers are fully suppressed, leaving a flat region safe for
/// spawning the player in chunk (0, 0, 0).
const SPAWN_FLAT_RADIUS: f64 = 48.0;

/// Radius (in blocks) at which the amplitude suppression fully fades out and the
/// terrain recovers full amplitude. Must be strictly greater than
/// [`SPAWN_FLAT_RADIUS`].
const SPAWN_BLEND_RADIUS: f64 = 256.0;

/// Number of coarsest octaves whose amplitude is suppressed near the spawn.
/// The remaining (finer) octaves are left untouched so the spawn area still has
/// some small surface detail rather than being a perfectly flat plane.
const SPAWN_SUPPRESS_OCTAVES: usize = 3;

/// Octaves of 2D noise. `(grid_size_in_blocks, amplitude_weight)`.
/// Weights are normalized at runtime. Largest grid size first to ensure the most
/// coarse layer dominates the overall shape.
const OCTAVES: &[(u64, f64)] = &[
    (512, 1.0),
    (256, 0.5),
    (128, 0.25),
    (64, 0.125),
    (32, 0.0625),
];

#[no_mangle]
pub extern "C" fn generate_chunk(seed: u64, phase: u64, chunk_x: i32, chunk_y: i32, chunk_z: i32) {
    let blocks_in_chunk_edge = api::get_blocks_in_chunk_edge();
    let bice_i64 = blocks_in_chunk_edge as i64;

    let empty = api::block_class!(empty);
    let grass = api::block_class!(grass);
    let stone = api::block_class!(stone);

    let air = api::block_environment!(air);
    let water = api::block_environment!(water);

    // Per-octave seeds derived from the world seed + phase so the layers are
    // decorrelated but reproducible.
    let octave_seeds: Vec<u64> = OCTAVES
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let mut hasher = Hasher64::new(seed);
            hasher.write(&phase.to_le_bytes());
            hasher.write(&(i as u64).to_le_bytes());
            hasher.finish()
        })
        .collect();

    let weight_sum: f64 = OCTAVES.iter().map(|(_, w)| *w).sum();

    // Precompute terrain height per (block_x, block_y) column in this chunk.
    let mut heights = [0i64; 32 * 32];
    debug_assert!((blocks_in_chunk_edge as usize) <= 32);

    for block_y in 0 .. blocks_in_chunk_edge {
        for block_x in 0 .. blocks_in_chunk_edge {
            // Global block (x, y) coordinates relative to the world origin.
            let global_x = chunk_x as i64 * bice_i64 + block_x as i64;
            let global_y = chunk_y as i64 * bice_i64 + block_y as i64;

            // Distance from the world origin in the x/y plane (in blocks).
            let dist = ((global_x as f64).powi(2) + (global_y as f64).powi(2)).sqrt();

            // `spawn_factor` is 0 inside the flat radius (terrain fully suppressed
            // for the coarsest octaves) and 1 outside the blend radius (terrain
            // fully restored). Between the two radii we smoothstep so the
            // transition is gradual and there are no visible seams.
            let spawn_factor = if dist <= SPAWN_FLAT_RADIUS {
                0.0
            } else if dist >= SPAWN_BLEND_RADIUS {
                1.0
            } else {
                let t = (dist - SPAWN_FLAT_RADIUS)
                    / (SPAWN_BLEND_RADIUS - SPAWN_FLAT_RADIUS);
                // Smootherstep: 6t^5 - 15t^4 + 10t^3
                t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
            };

            let mut value = 0.0;

            for (i, (grid_size, weight)) in OCTAVES.iter().enumerate() {
                // Scale down the coarsest octaves close to origin so the spawn
                // chunk is relatively smooth and close to water level.
                let octave_scale = if i < SPAWN_SUPPRESS_OCTAVES {
                    spawn_factor
                } else {
                    1.0
                };

                value += noise_2d(
                    octave_seeds[i],
                    *grid_size,
                    blocks_in_chunk_edge,
                    [chunk_x, chunk_y],
                    [block_x, block_y],
                ) * *weight
                    * octave_scale;
            }

            // `noise_2d` returns values roughly in [-1, 1]; normalize the weighted sum.
            let normalized = (value / weight_sum).clamp(-1.0, 1.0);

            // Terrain height as a global block z coordinate. The water level is
            // at global z = 0 (i.e. the boundary between chunk_z = -1 and chunk_z = 0).
            let height = (normalized * AMPLITUDE).round() as i64;

            heights[(block_y * blocks_in_chunk_edge + block_x) as usize] = height;
        }
    }

    // Decide whether empty cells in this chunk are water or air based on the
    // chunk's vertical position. Anything in a chunk with chunk_z < 0 sits below
    // the water surface and gets filled with water; everything else is air.
    let empty_block = if chunk_z < 0 { water } else { air };

    for block_z in 0 .. blocks_in_chunk_edge {
        let global_z = chunk_z as i64 * bice_i64 + block_z as i64;

        for block_y in 0 .. blocks_in_chunk_edge {
            for block_x in 0 .. blocks_in_chunk_edge {
                let height = heights[(block_y * blocks_in_chunk_edge + block_x) as usize];

                let data = if global_z > height {
                    // Above the terrain surface: water below sea level, air above.
                    pack_block_data(empty, empty_block, 0)
                } else if global_z > height - GRASS_DEPTH {
                    // Top layer of the column. Cover hills (above water) in grass,
                    // and leave the sea floor as stone.
                    if height >= 0 {
                        pack_block_data(grass, empty_block, 0)
                    } else {
                        pack_block_data(stone, empty_block, 0)
                    }
                } else {
                    // Deeper than the grass cap: stone.
                    pack_block_data(stone, empty_block, 0)
                };

                api::push_block(data);
            }
        }
    }
}
