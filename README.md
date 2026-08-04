# PixelBuddy

A desktop and WebAssembly pixel art editor written in Rust using `egui` and `glow` (OpenGL ES 3.0 / WebGL2).

[Live Web Demo](https://rowrow620.github.io/PixelBuddy/)

---

## Features

- **Layer Management & Blending**: Multi-layer document support with visibility toggles, opacity control, and composited blend modes (`Normal`, `Multiply`, `Screen`, `Overlay`).
- **Drawing Tools**:
  - **Pencil**: Pixel-perfect drawing algorithm that removes L-shaped corner pixels on 1px strokes.
  - **Eraser**: Transparency eraser preserving alpha channels.
  - **Bresenham Line & Shapes**: Line drawing, rectangle (filled and outline), and midpoint ellipse rendering.
  - **Flood Fill**: BFS flood fill with configurable per-channel color tolerance and contiguous mode toggling.
  - **Eyedropper**: Sampling of composited canvas colors.
- **Undo / Redo History**: Reversible command pattern stack storing pixel deltas instead of full canvas snapshots.
- **Cross-Platform Asynchronous File I/O**: Open and export PNG images on desktop and web browser targets via non-blocking channels.

---

## Supported Formats

- **PNG**: Full import and export support for 32-bit RGBA PNG image files across native desktop and WASM web builds.

---

## Stack

### Asynchronous File I/O

Browser environments disallow blocking the main thread during file picker execution. PixelBuddy handles file operations through a non-blocking message-passing channel (`crossbeam-channel`) combined with `rfd` and `wasm-bindgen-futures`. File dialog tasks execute concurrently on background threads (Desktop) or JavaScript promises (WASM) and send results back to the render loop.

```
[UI Trigger] ---> [Async Task / Promise] ---> [rfd File Dialog]
                                                      |
                                                (File Selection)
                                                      |
[Render Loop] <--- [crossbeam-channel] <--------------+
```

### Layer Compositing

The compositing engine calculates software layer flattening using RGBA alpha compositing before uploading the texture to the GPU:

$$\text{Alpha}_{\text{out}} = a_{\text{top}} + a_{\text{base}} \times (1 - a_{\text{top}})$$

$$\text{Color}_{\text{out}} = \frac{C_{\text{top}} \times a_{\text{top}} + C_{\text{base}} \times a_{\text{base}} \times (1 - a_{\text{top}})}{\text{Alpha}_{\text{out}}}$$

### Command Pattern History Stack

Drawing operations return a vector of modified pixel coordinates (`PixelChange`). The state manager packages changes into reversible `DrawCommand` structs on dual stacks (`undo_stack` and `redo_stack`).

---

## Building and Running

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) 1.80+
- [Trunk](https://trunkrs.dev/) (for web builds)

### Native Desktop Build

```bash
git clone https://github.com/rowrow620/PixelBuddy.git
cd PixelBuddy
cargo run --release
```

### WebAssembly Build

```bash
cargo install trunk
trunk serve
```

Access the application at `http://127.0.0.1:8080`.

---

## Dependencies

- **egui / eframe**: Immediate mode user interface library.
- **glow**: OpenGL ES 3.0 and WebGL2 bindings.
- **image**: PNG encoding and decoding.
- **rfd**: Native and web file dialogs.
- **crossbeam-channel**: Thread and task communication.

---

## License

MIT
