pub mod hash;

pub use paste::paste;

pub type BlockClass = u16;
pub type BlockEnvironment = u8;

mod import {
    extern "C" {
        pub fn get_blocks_in_chunk_edge() -> u32;
        pub fn get_block_class(ptr: *const u8, len: u32) -> u32;
        pub fn get_block_environment(ptr: *const u8, len: u32) -> u32;
        pub fn push_block(block_data: u32);
    }
}

/// Get the number of blocks in chunk edge from the host.
pub fn get_blocks_in_chunk_edge() -> u32 {
    unsafe { import::get_blocks_in_chunk_edge() }
}

/// Get a block class id by label name from the host.
pub fn get_block_class(label: &str) -> BlockClass {
    unsafe { import::get_block_class(label.as_ptr(), label.len() as u32) as BlockClass }
}

/// Get a block environment id by label name from the host.
pub fn get_block_environment(label: &str) -> BlockEnvironment {
    unsafe { import::get_block_environment(label.as_ptr(), label.len() as u32) as BlockEnvironment }
}

/// Push a packed block (see [`pack_block_data`]) to the host.
pub fn push_block(block_data: u32) {
    unsafe { import::push_block(block_data) }
}

/// Pack `(block_class, block_environment, block_metadata)` into a single `u32`
/// to be passed to the host via [`push_block`].
pub fn pack_block_data(
    block_class: BlockClass,
    block_environment: BlockEnvironment,
    block_metadata: u8,
) -> u32 {
    (block_class as u32) << 16 | (block_environment as u32) << 8 | block_metadata as u32
}

/// Look up a [`BlockClass`] by its label, caching the result for subsequent calls.
#[macro_export]
macro_rules! block_class {
    ($name:ident) => {{
        $crate::paste! {
            static [<$name:upper _NAME>]: &'static str = stringify!($name);
            thread_local!(static [<$name:upper>]: $crate::BlockClass = const { $crate::BlockClass::MAX });
            [<$name:upper>].with(|_| $crate::get_block_class([<$name:upper _NAME>]))
        }
    }};
}

/// Look up a [`BlockEnvironment`] by its label, caching the result for subsequent calls.
#[macro_export]
macro_rules! block_environment {
    ($name:ident) => {{
        $crate::paste! {
            static [<$name:upper _NAME>]: &'static str = stringify!($name);
            thread_local!(static [<$name:upper>]: $crate::BlockEnvironment = const { $crate::BlockEnvironment::MAX });
            [<$name:upper>].with(|_| $crate::get_block_environment([<$name:upper _NAME>]))
        }
    }};
}

// Extrasmoothstep for [0; 1.0]
pub fn interpolate(v1: f64, v2: f64, c: f64) -> f64 {
    (v2 - v1) * ((c * (c * 6.0 - 15.0) + 10.0) * c * c * c) + v1
}

pub fn noise_2d(
    seed: u64,
    grid_size: u64,
    blocks_in_chunk_edge: u32,
    chunk: [i32; 2],
    block: [u32; 2],
) -> f64 {
    let blocks_in_chunk_edge = blocks_in_chunk_edge as u64;

    let grid_coords = [0, 1].map(|axis| {
        let block_global =
            chunk[axis].abs_diff(i32::MIN) as u64 * blocks_in_chunk_edge + block[axis] as u64;
        let grid_coord_0 = block_global / grid_size;
        let grid_offset_0 =
            ((block_global - grid_coord_0 * grid_size) as f64 + 0.5) / grid_size as f64;

        let grid_coord_1 = grid_coord_0 + 1;
        let grid_offset_1 = grid_offset_0 - 1.0;

        [(grid_coord_0, grid_offset_0), (grid_coord_1, grid_offset_1)]
    });

    let interp_coefs_by_axis = grid_coords.map(|grid_coords| grid_coords[0].1);

    let hasher = hash::Hasher64::new(seed);

    let mut grid_iter = [0, 1].into_iter().flat_map(move |a1| {
        [0, 1].into_iter().map(move |a0| {
            (
                [grid_coords[0][a0].0, grid_coords[1][a1].0],
                [grid_coords[0][a0].1, grid_coords[1][a1].1],
            )
        })
    });

    let dot_products = [(); 4].map(|_| {
        let (grid_coords, grid_offset) = grid_iter.next().unwrap();

        let mut hasher = hasher.clone();

        grid_coords
            .iter()
            .for_each(|i| hasher.write(&i.to_le_bytes()));

        let hashed_bytes = hasher.finish().to_le_bytes();

        let gradient_x = u32::from_le_bytes(hashed_bytes[.. 4].try_into().unwrap()) as f64
            / u32::MAX as f64
            * 2.0
            - 1.0;

        let mut gradient_y = (1.0 - gradient_x * gradient_x).sqrt();

        if i32::from_le_bytes(hashed_bytes[4 ..].try_into().unwrap()).is_negative() {
            gradient_y = -gradient_y;
        }

        grid_offset[0] * gradient_x + grid_offset[1] * gradient_y
    });

    // Suming by X (axis 0)
    let mut axis_iter = dot_products.into_iter();
    let dot_products = [(); 2].map(|_| {
        let (value_0, value_1) = (axis_iter.next().unwrap(), axis_iter.next().unwrap());

        let coef = interp_coefs_by_axis[0];

        interpolate(value_0, value_1, coef)
    });

    // Suming by Y (axis 1)
    let mut axis_iter = dot_products.into_iter();
    let dot_product = {
        let (value_0, value_1) = (axis_iter.next().unwrap(), axis_iter.next().unwrap());

        let coef = interp_coefs_by_axis[1];

        interpolate(value_0, value_1, coef)
    };

    dot_product
}

pub fn noise_3d(
    seed: u64,
    grid_size: u64,
    blocks_in_chunk_edge: u32,
    chunk: [i32; 3],
    block: [u32; 3],
) -> f64 {
    let blocks_in_chunk_edge = blocks_in_chunk_edge as u64;

    let grid_coords = [0, 1, 2].map(|axis| {
        let block_global =
            chunk[axis].abs_diff(i32::MIN) as u64 * blocks_in_chunk_edge + block[axis] as u64;
        let grid_coord_0 = block_global / grid_size;
        let grid_offset_0 =
            ((block_global - grid_coord_0 * grid_size) as f64 + 0.5) / grid_size as f64;

        let grid_coord_1 = grid_coord_0 + 1;
        let grid_offset_1 = grid_offset_0 - 1.0;

        [(grid_coord_0, grid_offset_0), (grid_coord_1, grid_offset_1)]
    });

    let interp_coefs_by_axis = grid_coords.map(|grid_coords| grid_coords[0].1);

    let hasher = hash::Hasher64::new(seed);

    let mut grid_iter = [0, 1].into_iter().flat_map(|a2| {
        [0, 1].into_iter().flat_map(move |a1| {
            [0, 1].into_iter().map(move |a0| {
                (
                    [
                        grid_coords[0][a0].0,
                        grid_coords[1][a1].0,
                        grid_coords[2][a2].0,
                    ],
                    [
                        grid_coords[0][a0].1,
                        grid_coords[1][a1].1,
                        grid_coords[2][a2].1,
                    ],
                )
            })
        })
    });

    let dot_products = [(); 8].map(|_| {
        let (grid_coords, grid_offset) = grid_iter.next().unwrap();

        let mut hasher = hasher.clone();

        grid_coords
            .iter()
            .for_each(|i| hasher.write(&i.to_le_bytes()));

        let hashed_bytes = hasher.finish().to_le_bytes();

        let gradient_x = u32::from_le_bytes(hashed_bytes[.. 4].try_into().unwrap()) as f64
            / u32::MAX as f64
            * 2.0
            - 1.0;

        let gradient_y = u32::from_le_bytes(hashed_bytes[4 ..].try_into().unwrap()) as f64
            / u32::MAX as f64
            * 2.0
            - 1.0;

        hasher.write(&hasher.finish().to_le_bytes());
        let hashed_bytes = hasher.finish().to_le_bytes();

        let gradient_z = u32::from_le_bytes(hashed_bytes[.. 4].try_into().unwrap()) as f64
            / u32::MAX as f64
            * 2.0
            - 1.0;

        let gradient_mag =
            (gradient_x * gradient_x + gradient_y * gradient_y + gradient_z * gradient_z).sqrt();
        let gradient = if gradient_mag == 0.0 {
            let mut vec = [0.0; 3];
            let decide = u32::from_le_bytes(hashed_bytes[4 ..].try_into().unwrap()) as f64
                / u32::MAX as f64
                * 3.0;
            if decide < 1.0 {
                vec[0] = 1.0;
            } else if decide < 2.0 {
                vec[1] = 1.0;
            } else {
                vec[2] = 1.0;
            }

            vec
        } else {
            [
                gradient_x / gradient_mag,
                gradient_y / gradient_mag,
                gradient_z / gradient_mag,
            ]
        };

        grid_offset[0] * gradient[0] + grid_offset[1] * gradient[1] + grid_offset[2] * gradient[2]
    });

    // Suming by X (axis 0)
    let mut axis_iter = dot_products.into_iter();
    let dot_products = [(); 4].map(|_| {
        let (value_0, value_1) = (axis_iter.next().unwrap(), axis_iter.next().unwrap());

        let coef = interp_coefs_by_axis[0];

        interpolate(value_0, value_1, coef)
    });

    // Suming by Y (axis 1)
    let mut axis_iter = dot_products.into_iter();
    let dot_products = [(); 2].map(|_| {
        let (value_0, value_1) = (axis_iter.next().unwrap(), axis_iter.next().unwrap());

        let coef = interp_coefs_by_axis[1];

        interpolate(value_0, value_1, coef)
    });

    // Suming by Z (axis 2)
    let mut axis_iter = dot_products.into_iter();
    let dot_product = {
        let (value_0, value_1) = (axis_iter.next().unwrap(), axis_iter.next().unwrap());

        let coef = interp_coefs_by_axis[2];

        interpolate(value_0, value_1, coef)
    };

    dot_product
}

