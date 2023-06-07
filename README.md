# realtime-rc

Realtime camera streaming and motor control via WebRTC.

## Requirements

- GCC/Clang
  - Ubuntu/Debian: `build-essential libclang-dev`
  - Fedora/CentOS: `make automake gcc gcc-c++ kernel-devel clang-devel`
  - Arch Linux: `base-devel`
- Rust 1.65.0 or newer, see: https://www.rust-lang.org/tools/install
- Cargo (usually included with Rust)
- Git

## Installation

1. Clone this repository: `git clone https://github.com/mat-sz/realtime-rc.git`.
2. If using rustup, install and enable the `stable` toolchain: https://rust-lang.github.io/rustup/concepts/toolchains.html

## Running

### Development

```
cargo run
```

### Release

```
cargo build --release
```

The executable file will be located at: `./target/release/realtime-rc`

### Help

```
cargo run -- --help
```
