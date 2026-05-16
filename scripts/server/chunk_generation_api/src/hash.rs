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
