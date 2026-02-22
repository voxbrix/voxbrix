# Cross Compilation

Collection of general advice regarding cross compilation of crates in this repository.

## Prerequisites

- A working [Rust toolchain](https://rustup.rs/) installed via `rustup`.
- The appropriate cross-compilation target added via `rustup target add <host_target>`.

## macOS aarch64 (from Linux x86_64)

**Required tools:** `clang`, `lld`.

### Setup

1. Add `rust-lld` to `$PATH`. Replace the host toolchain name if yours differs:

   ```sh
   export PATH="$PATH:$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-musl/lib/rustlib/x86_64-unknown-linux-musl/bin"
   ```

2. Add the cross compilation target:

   ```sh
   rustup target add aarch64-apple-darwin
   ```

3. Obtain a macOS SDK and note its path (referred to as `/path/to/macos_sdk` below).

### Build

```sh
export CC=clang
export SDKROOT=/path/to/macos_sdk
export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=rust-lld
cargo build --release --target aarch64-apple-darwin -p voxbrix_client
```
