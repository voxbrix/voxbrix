# Voxbrix Protocol
Voxbrix uses custom protocol for client-server communication. Main reason for implementing this, instead of using either TCP or UDP, is need in both reliable and unreliable messages.  
  
The protocol is a relatively thin layer over UDP. To authenticate messages (and to increase privacy a bit) the ChaCha20-Poly1305 is used, with secp256k1 ECDH handshake.  
  
Generally, messages have the following structure:
```
| sender_id: Id | type: u8 | ...other fields... |
```
  
`sender_id` for server is `0`, for client trying to connect is `1` and for connected clients it is assigned by the server.
  
Other fields depend on the `type`:
```
    const CONNECT: u8 = 0;
        // key: Key,

    const ACCEPT: u8 = 1;
        // key: Key,
        // id: Id,

    const ACKNOWLEDGE: u8 = 2;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // sequence: Sequence,

    const DISCONNECT: u8 = 3;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],

    const UNRELIABLE: u8 = 4;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // data: &[u8],

    const UNRELIABLE_SPLIT_START: u8 = 5;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // split_id: SplitId,
        // length: u32,
        // data: &[u8],

    const UNRELIABLE_SPLIT: u8 = 6;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // split_id: SplitId,
        // count: u32,
        // data: &[u8],

    const RELIABLE: u8 = 7;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // sequence: Sequence,
        // data: &[u8],

    const RELIABLE_SPLIT: u8 = 8;
        // tag: [u8; TAG_SIZE],
        // nonce: [u8; NONCE_SIZE],
        // encrypted fields:
        // channel: Channel,
        // sequence: Sequence,
        // data: &[u8],
```
  
Types used are either integers or byte arrays/slices. Integers are encoded with variable length integer encoding (by using `integer-encoding` crate), which is used in Google's Protocol Buffers.
  
## Authenticated Encryption With Associated Data
To form a new connection ECDH handshake is used:
1. Client sends `CONNECTION` message, which is unencryped and consists of client's ephemeral public key;
2. In reponse, server sends `ACCEPT` message - also unencrypted and has server's ephemeral public key, but also assigns `id` the to client.  

The secret for ChaCha20-Poly1305 is derived from the result secret of the ECDH key exchange.  
In the code quote above, the encrypted data is below `// encrypted fields:`. Encoded sender id and type of the message are used as Associated Data. Nonce and tag are plain, unencrypted byte arrays.

## Security notes

* As-is the protocol is obviously vulnerable to MITM, however with authentication (e.g. in form of ecdsa signatures for the public ephemeral keys) should provide enough security.
* Unreliable messages must **NOT** be used for anything non-idempotent, they are not protected against duplication.
* In case you see any holes in it, please [open an issue](https://codeberg.org/voxbrix/voxbrix/issues).
