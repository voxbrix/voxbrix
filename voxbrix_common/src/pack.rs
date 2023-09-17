use bincode::Options;
use lz4_flex::block as lz4;
use serde::{
    de::Deserialize,
    Serialize,
};
use std::io::Write;

const COMPRESS_LENGTH: usize = 100;
// TODO have this separate for client and server:
const MAX_UNCOMPRESSED_BYTES: usize = 100_000_000;

fn packer() -> impl Options {
    bincode::options()
}

/// Low level packer with default config.
pub fn serialize_into<T>(value: &T, buffer: &mut Vec<u8>)
where
    T: Serialize,
{
    let size = packer()
        .serialized_size(value)
        .expect("unable to calculate size for the serialized object") as usize;

    buffer.clear();
    buffer.reserve(size);

    packer()
        .serialize_into(buffer, value)
        .expect("unable to serialize value");
}

/// Low level unpacker with default config.
pub fn deserialize_from<'a, T>(buffer: &'a [u8]) -> Option<T>
where
    T: Deserialize<'a>,
{
    packer().deserialize(buffer).ok()
}

#[derive(Debug)]
pub struct UnpackError;

pub trait Pack {
    const DEFAULT_COMPRESSED: bool;
}

pub struct Packer {
    buffer: Vec<u8>,
}

impl Packer {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn pack_uncompressed<T>(&self, data: &T, output: &mut Vec<u8>)
    where
        T: Serialize,
    {
        output.clear();
        packer()
            .serialize_into(output, &data)
            .expect("serialization error");
    }

    pub fn pack_compressed<T>(&mut self, data: &T, output: &mut Vec<u8>)
    where
        T: Serialize,
    {
        output.clear();
        self.buffer.clear();

        let serialized_size = packer()
            .serialized_size(&data)
            .expect("serialized size calculation error") as usize;

        if serialized_size > COMPRESS_LENGTH {
            packer()
                .serialize_into(&mut self.buffer, &data)
                .expect("serialization error");

            let uncompressed_len = self.buffer.len();

            // 1 is compression flag, 4 is prepended uncompressed size
            let max_output_size = 5 + lz4::get_maximum_output_size(uncompressed_len);

            output.reserve(max_output_size);

            unsafe {
                output.set_len(max_output_size);
            }

            output[0] = 1;
            output[1 .. 5].copy_from_slice(&(uncompressed_len as u32).to_le_bytes());
            let len = lz4::compress_into(self.buffer.as_ref(), &mut output[5 ..]).unwrap();
            output.truncate(5 + len);
        } else {
            output.write_all(&[0]).expect("serialization error");

            packer()
                .serialize_into(output, data)
                .expect("serialization error");
        }
    }

    pub fn pack_uncompressed_to_vec<T>(&mut self, data: &T) -> Vec<u8>
    where
        T: Serialize,
    {
        packer().serialize(data).expect("serialization error")
    }

    pub fn pack_compressed_to_vec<T>(&mut self, data: &T) -> Vec<u8>
    where
        T: Serialize,
    {
        let mut output = Vec::new();
        self.pack_compressed(data, &mut output);
        output
    }

    pub fn unpack_uncompressed<'a, T>(&self, input: &'a [u8]) -> Result<T, UnpackError>
    where
        T: Deserialize<'a>,
    {
        packer().deserialize::<T>(input).map_err(|_| UnpackError)
    }

    pub fn unpack_compressed<'a, T>(&'a mut self, input: &'a [u8]) -> Result<T, UnpackError>
    where
        T: Deserialize<'a>,
    {
        let input = input.as_ref();

        match input.first() {
            Some(0) => {
                packer()
                    .deserialize::<T>(&input[1 ..])
                    .map_err(|_| UnpackError)
            },
            Some(1) => {
                let size = u32::from_le_bytes(input[1 .. 5].try_into().unwrap()) as usize;

                if size > MAX_UNCOMPRESSED_BYTES {
                    return Err(UnpackError);
                }

                self.buffer.clear();
                self.buffer.reserve(size as usize);
                unsafe {
                    self.buffer.set_len(size);
                }

                let actual_size = lz4::decompress_into(&input[5 ..], self.buffer.as_mut())
                    .map_err(|_| UnpackError)?;

                if actual_size != size {
                    return Err(UnpackError);
                }

                packer()
                    .deserialize::<T>(&self.buffer)
                    .map_err(|_| UnpackError)
            },
            _ => Err(UnpackError),
        }
    }

    pub fn pack<T>(&mut self, data: &T, output: &mut Vec<u8>)
    where
        T: Serialize + Pack,
    {
        output.clear();

        if T::DEFAULT_COMPRESSED {
            self.pack_compressed(data, output)
        } else {
            self.pack_uncompressed(data, output)
        }
    }

    pub fn pack_to_vec<T>(&mut self, data: &T) -> Vec<u8>
    where
        T: Serialize + Pack,
    {
        if T::DEFAULT_COMPRESSED {
            self.pack_compressed_to_vec(data)
        } else {
            self.pack_uncompressed_to_vec(data)
        }
    }

    pub fn unpack<'a, T>(&'a mut self, input: &'a [u8]) -> Result<T, UnpackError>
    where
        T: Deserialize<'a> + Pack,
    {
        if T::DEFAULT_COMPRESSED {
            self.unpack_compressed(input)
        } else {
            self.unpack_uncompressed(input)
        }
    }
}
