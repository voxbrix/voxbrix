use hash::Hasher64;
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
pub extern "C" fn generate_chunk(seed: u64, phase: u64, chunk_x: i32, chunk_y: i32, chunk_z: i32) {
    let blocks_in_chunk_edge = unsafe {
        static mut BICE: Option<u32> = None;
        if BICE.is_none() {
            BICE = Some(get_blocks_in_chunk_edge())
        }
        BICE.unwrap()
    };

    let air = block_class!(air);
    let grass = block_class!(grass);
    let stone = block_class!(stone);

    let push_block = |block_class: u64| unsafe {
        push_block(block_class);
    };

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

                let ground_block_z = blocks_in_chunk_edge - 1;

                if chunk_z % 32 == 0 && (0 ..= ground_block_z).contains(&block_z) {
                    let width_coef = block_z as f64 / ground_block_z as f64;

                    let block_value = (1.0 - block_value.abs()) * (0.8 + 0.2 * width_coef);

                    if block_value > 0.95 {
                        push_block(grass);
                    } else {
                        push_block(air);
                    }
                } else {
                    push_block(air);
                }
            }
        }
    }
}

fn noise_2d(
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

    let hasher = Hasher64::new(seed);

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

fn noise_3d(
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

    let hasher = Hasher64::new(seed);

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

// Extrasmoothstep for [0; 1.0]
fn interpolate(v1: f64, v2: f64, c: f64) -> f64 {
    (v2 - v1) * ((c * (c * 6.0 - 15.0) + 10.0) * c * c * c) + v1
}

mod hash {
    // Fast u64-producing algorithm, basically FxHash, but reimplemented to output u64 instead of
    // padded usize.

    use core::ops::BitXor;

    #[derive(Clone)]
    pub struct Hasher64(u64);

    const K: u64 = 0x517cc1b727220a95;

    impl Hasher64 {
        #[inline]
        pub fn new(seed: u64) -> Self {
            Self(seed)
        }

        #[inline]
        fn push(&mut self, i: u64) {
            self.0 = self.0.rotate_left(5).bitxor(i).wrapping_mul(K);
        }

        #[inline]
        pub fn write(&mut self, mut bytes: &[u8]) {
            // TODO should we really have `from_ne_bytes` here?
            while bytes.len() >= 8 {
                self.push(u64::from_le_bytes(bytes[.. 8].try_into().unwrap()));
                bytes = &bytes[8 ..];
            }
            if bytes.len() >= 4 {
                self.push(u32::from_le_bytes(bytes[.. 4].try_into().unwrap()) as u64);
                bytes = &bytes[4 ..];
            }
            if bytes.len() >= 2 {
                self.push(u16::from_le_bytes(bytes[.. 2].try_into().unwrap()) as u64);
                bytes = &bytes[2 ..];
            }
            if bytes.len() >= 1 {
                self.push(bytes[0] as u64);
            }
        }

        #[inline]
        pub fn finish(&self) -> u64 {
            self.0 as u64
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        pub fn test_endianness() {
            //  Even though the WASM is little-endian only, the hasher could later
            //  be a separate crate, so endianness-independency is worth checking.
            //
            //  Test big-endian:
            //  cargo +nightly miri test --target s390x-unknown-linux-gnu
            //
            //  Test 32-bit:
            //  cargo +nightly miri test --target i686-unknown-linux-gnu

            let mut hash = Hasher64::new(5);

            hash.write(b"test_string");

            assert_eq!(hash.finish(), 3138908053291983918);

            let mut hash = Hasher64::new(3957196563549288);

            let mut string = "test_string".to_owned();
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_very");
            string.push_str("_very_very_very_very_very_very_very_long");

            hash.write(string.as_bytes());

            assert_eq!(hash.finish(), 9946340679755297201);
        }
    }
}
