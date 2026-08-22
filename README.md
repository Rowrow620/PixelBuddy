# PixelBuddy

[![Play Online in Browser](https://img.shields.io/badge/Play_Online-WebAssembly-blue?style=for-the-badge&logo=webassembly)](https://rowrow620.github.io/PixelBuddy/)

<img width="1172" height="938" alt="{4F60DFA3-7A82-482D-8406-C7A57A6549A8}" src="https://github.com/user-attachments/assets/dee7deda-a30d-44b5-a5df-e4d5376af8a3" />

PixelBuddy is a work-in-progress Pixel Art software tool for creating, editing, and managing pixel art sprites and animations. It is available to use instantly in your browser through WebAssembly, or downloadable as a native desktop application. 

Built using Rust.

<img width="1186" height="944" alt="{E7DFB33A-76ED-4F09-92D7-971C3B5BB243}" src="https://github.com/user-attachments/assets/3e615fe0-0396-4284-8b0f-13267a052d8a" />

## Development and release checks

PixelBuddy's documented minimum Rust version is **1.88**. Before release, run:

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
cargo build --release --all-features --locked
cargo check --target wasm32-unknown-unknown --tests --all-features --locked
```

CI repeats these checks and runs a scheduled dependency, license, and source audit. See [Security, Resource Limits, and Recovery](docs/SECURITY_AND_RECOVERY.md), [Security Policy](SECURITY.md), and [Third-Party Notices](THIRD_PARTY_NOTICES.md).
