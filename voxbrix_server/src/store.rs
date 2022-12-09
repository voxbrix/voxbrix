#[derive(Debug)]
pub enum FromKeyError {
    IncorrectBufferSize,
}

#[derive(Debug)]
pub enum ToKeyError {
    BufferTooSmall,
}

pub trait AsKey {
    fn to_key(&self, buf: &mut [u8]) -> Result<(), ToKeyError>;
    fn from_key<B>(buf: B) -> Result<Self, FromKeyError>
    where
        Self: Sized,
        B: AsRef<[u8]>;
}
