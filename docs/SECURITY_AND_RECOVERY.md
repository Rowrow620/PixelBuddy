# Security, Resource Limits, and Recovery

PixelBuddy treats `.pbud`, raster images, sprite sheets, and local recovery values as untrusted input. Limits are checked before or while decoding so malformed data fails with a user-facing error instead of creating unbounded allocations.

## Enforced limits

| Resource | Limit |
|---|---:|
| Encoded raster input | 64 MiB |
| Canvas edge | 8,192 pixels |
| Pixels in one canvas | 16,777,216 |
| Editable project file | 256 MiB |
| Decoded RGBA data across a project | 256 MiB |
| Recovery snapshot | 32 MiB |
| Animation frames in a project | 4,096 |
| Imported sprite-sheet frames | 1,024 |
| Aggregate pixels produced by one sprite-sheet import | 8,388,608 |
| Sprite-sheet preview texture | 256 × 256 |
| Layers per frame | 256 |
| Palette colors | 256 |
| Animation tags | 1,024 |
| Tag name | 64 characters and 128 UTF-8 bytes |
| Layer name | 256 UTF-8 bytes |
| Undo/redo retained data | 64 MiB combined |
| Timeline thumbnail textures | 512-entry LRU |
| Tile-preview copies | 15 per axis |

Dimension and allocation arithmetic uses checked operations. Encode and decode share project validation, so PixelBuddy will not knowingly write metadata it would reject when reopening.

## Recovery lifecycle

Recovery is a single eframe storage value named `pixelbuddy.recovery.v1` (native application storage on desktop and browser storage on WebAssembly). While a project is dirty, the application refreshes the value during the existing 20-second autosave cycle and on application save callbacks. A clean save or explicit discard clears the old recovery value. Empty, corrupt, truncated, and values over 32 MiB are ignored or rejected without replacing the active project.

A valid recovery is offered rather than applied automatically. Restoring it uses the same dirty-document confirmation boundary as New/Open/Import. The recovery value is consumed only after the user confirms the replacement; cancellation preserves both the current project and the recovery candidate.

Recovery is best-effort crash protection, not a substitute for `.pbud` saves or backups. Browsers and operating systems may evict application storage.

## Known advisory disposition

`RUSTSEC-2026-0192` reports that `ttf-parser` is unmaintained and lists no patched versions. PixelBuddy receives `ttf-parser 0.25.1` transitively through `owned_ttf_parser` → `ab_glyph` → `epaint`/`egui`; it is not a direct dependency. The advisory is informational rather than a reported vulnerability, and is narrowly ignored in `deny.toml` so all other advisories remain release-blocking. Dependabot and the scheduled dependency policy run make the exception visible when the graph changes; remove it when upstream egui adopts a maintained parser.

## Release checks

The pull-request CI gate runs formatting, strict Clippy, all native tests, an MSRV native release build, and a WebAssembly test/application check using Rust 1.88. The separate dependency workflow checks RustSec advisories, registry sources, and SPDX licenses. Deployment runs only after the quality job succeeds.
