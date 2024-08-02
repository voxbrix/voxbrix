use lz4_flex::block as lz4;
use postcard::ser_flavors::Flavor;
use serde::{
    Deserialize,
    Serialize,
};
use std::io::Write;

const COMPRESS_LENGTH: usize = 100;
// TODO have this separate for client and server:
const MAX_UNCOMPRESSED_BYTES: usize = 100_000_000;

struct Writer<W> {
    written: usize,
    writer: W,
}

impl<W> Flavor for Writer<W>
where
    W: Write,
{
    type Output = usize;

    fn try_push(&mut self, data: u8) -> postcard::Result<()> {
        self.writer
            .write_all(&[data])
            .map_err(|_| postcard::Error::SerializeBufferFull)?;
        self.written += 1;
        Ok(())
    }

    fn finalize(mut self) -> postcard::Result<Self::Output> {
        self.writer
            .flush()
            .map_err(|_| postcard::Error::SerializeBufferFull)?;
        Ok(self.written)
    }

    fn try_extend(&mut self, data: &[u8]) -> postcard::Result<()> {
        self.writer
            .write_all(data)
            .map_err(|_| postcard::Error::SerializeBufferFull)?;
        self.written += data.len();
        Ok(())
    }
}

/// Low level packer with default config, continues writing.
pub fn encode_write<T, W>(value: &T, writer: &mut W) -> usize
where
    T: Serialize,
    W: Write,
{
    postcard::serialize_with_flavor(value, Writer { written: 0, writer }).unwrap()
}

/// Low level packer with default config, resets the buffer before the write.
pub fn encode_into<T>(value: &T, buffer: &mut Vec<u8>)
where
    T: Serialize,
{
    buffer.clear();

    encode_write(value, buffer);
}

/// Low level unpacker with default config.
pub fn decode_from_slice<'a, T>(buffer: &'a [u8]) -> Option<(T, usize)>
where
    T: Deserialize<'a>,
{
    let (value, leftover) = postcard::take_from_bytes(buffer).ok()?;
    let bytes_read = buffer.len().saturating_sub(leftover.len());
    Some((value, bytes_read))
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
        encode_into(data, output)
    }

    pub fn pack_compressed<T>(&mut self, data: &T, output: &mut Vec<u8>)
    where
        T: Serialize,
    {
        output.clear();
        self.buffer.clear();

        encode_write(data, &mut self.buffer);

        if self.buffer.len() > COMPRESS_LENGTH {
            encode_write(&1u8, output);

            encode_write(&(self.buffer.len() as u64), output);

            let compressed_start = output.len();

            let compressed_max_size = lz4::get_maximum_output_size(self.buffer.len());

            output.reserve(compressed_max_size);

            unsafe {
                output.set_len(compressed_start + compressed_max_size);
            }

            let compressed_size = lz4::compress_into(
                self.buffer.as_ref(),
                &mut output.as_mut_slice()[compressed_start ..],
            )
            .unwrap();

            output.truncate(compressed_start + compressed_size);
        } else {
            encode_write(&0u8, output);
            output.extend_from_slice(self.buffer.as_slice());
        }
    }

    pub fn pack_uncompressed_to_vec<T>(&mut self, data: &T) -> Vec<u8>
    where
        T: Serialize,
    {
        let mut output = Vec::new();
        self.pack_uncompressed(data, &mut output);
        output
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
        Ok(decode_from_slice(input).ok_or(UnpackError)?.0)
    }

    pub fn unpack_compressed<'a, T>(&'a mut self, input: &'a [u8]) -> Result<T, UnpackError>
    where
        T: Deserialize<'a>,
    {
        match input.first() {
            Some(0) => {
                let (output, _) = decode_from_slice(&input[1 ..]).ok_or(UnpackError)?;

                Ok(output)
            },
            Some(1) => {
                let (size, offset) = decode_from_slice::<u64>(&input[1 ..]).ok_or(UnpackError)?;
                let start = offset + 1;
                let size: usize = size.try_into().map_err(|_| UnpackError)?;

                if size > MAX_UNCOMPRESSED_BYTES {
                    return Err(UnpackError);
                }

                self.buffer.clear();
                self.buffer.reserve(size as usize);
                unsafe {
                    self.buffer.set_len(size);
                }

                let actual_size = lz4::decompress_into(&input[start ..], self.buffer.as_mut())
                    .map_err(|_| UnpackError)?;

                if actual_size != size {
                    return Err(UnpackError);
                }

                let (output, _) = decode_from_slice(self.buffer.as_mut()).ok_or(UnpackError)?;

                Ok(output)
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
