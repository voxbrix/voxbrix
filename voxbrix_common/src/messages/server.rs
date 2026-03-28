use crate::{
    entity::snapshot::{
        ClientSnapshot,
        ServerSnapshot,
    },
    messages::{
        ClientActionsPacked,
        UpdatesPacked,
    },
    pack::{
        Pack,
        Packer,
        UnpackError,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize)]
pub struct ClientState<'a> {
    pub snapshot: ClientSnapshot,
    // last server's snapshot received by this client
    pub last_server_snapshot: ServerSnapshot,
    #[serde(borrow)]
    pub updates: UpdatesPacked<'a>,
    #[serde(borrow)]
    pub actions: ClientActionsPacked<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ServerAcceptKind {
    State = 0,
}

impl ServerAcceptKind {
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::State),
            _ => None,
        }
    }
}

/// Wire layout: [tag: u8 = ServerAcceptKind] [payload: pack_compressed_append].
pub struct ServerAcceptMessage {
    bytes: Vec<u8>,
}

impl ServerAcceptMessage {
    fn pack_with<T: Serialize>(packer: &mut Packer, kind: ServerAcceptKind, value: &T) -> Self {
        let mut bytes = Vec::new();
        bytes.push(kind as u8);
        packer.pack_compressed_append(value, &mut bytes);
        Self { bytes }
    }

    pub fn pack_state(packer: &mut Packer, value: &ClientState<'_>) -> Self {
        Self::pack_with(packer, ServerAcceptKind::State, value)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, UnpackError> {
        let tag = *bytes.first().ok_or(UnpackError)?;
        ServerAcceptKind::from_byte(tag).ok_or(UnpackError)?;
        Ok(Self { bytes })
    }

    pub fn from_slice(bytes: &[u8]) -> Result<ServerAcceptMessageRef<'_>, UnpackError> {
        let tag = *bytes.first().ok_or(UnpackError)?;
        ServerAcceptKind::from_byte(tag).ok_or(UnpackError)?;
        Ok(ServerAcceptMessageRef { bytes })
    }

    pub fn kind(&self) -> ServerAcceptKind {
        ServerAcceptKind::from_byte(self.bytes[0]).expect("tag byte validated on construction")
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn unpack_state<'a>(
        &'a self,
        packer: &'a mut Packer,
    ) -> Result<ClientState<'a>, UnpackError> {
        if self.kind() != ServerAcceptKind::State {
            return Err(UnpackError);
        }
        packer.unpack_compressed::<ClientState<'a>>(&self.bytes[1 ..])
    }
}

pub struct ServerAcceptMessageRef<'a> {
    bytes: &'a [u8],
}

impl<'a> ServerAcceptMessageRef<'a> {
    pub fn kind(&self) -> ServerAcceptKind {
        ServerAcceptKind::from_byte(self.bytes[0]).expect("tag byte validated on construction")
    }

    pub fn unpack_state<'b>(
        &'b self,
        packer: &'b mut Packer,
    ) -> Result<ClientState<'b>, UnpackError> {
        if self.kind() != ServerAcceptKind::State {
            return Err(UnpackError);
        }
        packer.unpack_compressed::<ClientState<'b>>(&self.bytes[1 ..])
    }
}

#[derive(Serialize, Deserialize)]
pub enum InitRequest {
    Login,
    Register,
}

impl Pack for InitRequest {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    #[serde(with = "serde_big_array::BigArray")]
    pub key_signature: [u8; 64],
}

impl Pack for LoginRequest {
    const DEFAULT_COMPRESSED: bool = false;
}

#[derive(Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    #[serde(with = "serde_big_array::BigArray")]
    pub public_key: [u8; 33],
}

impl Pack for RegisterRequest {
    const DEFAULT_COMPRESSED: bool = false;
}
