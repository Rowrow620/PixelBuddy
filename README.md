# PixelBuddy

PixelBuddy is a cross-platform pixel art editor built in Rust using `egui` and hardware-accelerated graphics. It runs natively on desktop platforms and compiles directly to WebAssembly for browser deployment.

## Setup & Running

### Desktop (Native)

```bash
git clone https://github.com/Rowrow620/PixelBuddy.git
cd PixelBuddy
cargo run --release
```

### Web (WASM)

```bash
cargo install trunk
trunk serve
```

Open `http://127.0.0.1:8080` in your web browser.
