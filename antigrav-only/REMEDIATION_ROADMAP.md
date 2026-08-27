# PixelBuddy Remediation Roadmap

Created: 2026-08-20  
Last verified: 2026-08-25
Status: Active — Milestones 0 through 4 are complete for their audited scope. Milestone 5 is implemented locally but still awaits its first pushed hosted-runner confirmation. Milestone 6 contains prototypes for all thirteen planned effects and most palette infrastructure. PB-018 and PB-019 are complete; PB-017 awaits final transaction/UI regressions, and PB-020 modularization remains open.
Scope: Correctness, data safety, security hardening, performance, maintainability, and release hygiene

This document is the canonical local engineering audit and remediation plan. It records the current findings and orders the technical work required to make further feature development safer. Older architecture, audit, and product-planning snapshots were removed because significant codebase changes made their inventories and baseline claims unreliable.

## Desired outcome

PixelBuddy should have one reliable mutation path for editor state, consistent dirty/history/cache behavior, bounded processing of untrusted files, and a clean automated quality gate. The highest-risk work is preventing silent data loss or edits being applied to the wrong frame.

## Current verified baseline

As of 2026-08-25:

- `cargo test --all-targets --all-features` passes: 221 tests.
- `cargo check --target wasm32-unknown-unknown --tests --all-features` passes.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --all-targets --all-features -- -D warnings` passes without lint suppressions.
- Local `main` is 27 commits ahead of `origin/main`; the hosted CI and release smoke gates have therefore not exercised this remediation series.
- Milestone 6 implementation and this roadmap update remain intentionally uncommitted.

## Priority and effort legend

| Label | Meaning |
|---|---|
| P0 | Possible data loss or corruption; fix before feature work |
| P1 | High-impact correctness, memory, or reliability risk |
| P2 | Important structural or defense-in-depth improvement |
| P3 | Cleanup with low immediate user impact |
| S | Up to one focused day |
| M | Roughly two to four focused days |
| L | Multi-part change; split into reviewable commits |

Effort estimates are relative and should be revised after the first implementation pass.

## Finding register

| ID | Priority | Status | Finding | Primary location | Effort | Milestone |
|---|---:|---|---|---|---:|---:|
| PB-001 | P0 | Complete | Settings presets replaced the document without the normal dirty-document confirmation | replacement command/UI | S | 1 |
| PB-002 | P0 | Complete | Frame shortcuts bypassed the editor transition path and retained cross-frame state | frame transition commands | S | 1 |
| PB-003 | P1 | Complete | Merge Down targets the wrong neighbor for the current layer ordering and needs explicit lock/compositing behavior | `src/app.rs` | M | 1 |
| PB-004 | P0 | Complete | Persisted mutations have incomplete dirty-state tracking; resize and some structural layer paths remain | app/editor/timeline mutation paths | M | 1 |
| PB-005 | P0 | Complete | Global shortcuts remain active while a text field has focus | `src/app.rs` shortcut dispatch | M | 1 |
| PB-006 | P1 | Complete | Sprite-sheet import enforces source-byte, decoded-pixel, aggregate-pixel, and frame-count budgets before allocation | sprite-sheet I/O/import UI | M | 3 |
| PB-007 | P1 | Complete | Undo, redo, and suspended futures share count and retained-byte budgets with oldest-transaction eviction | history/editor state | L | 3 |
| PB-008 | P1 | Complete | Timeline thumbnails are fixed-size, visible-cell lazy, revision-aware, and capped by an LRU texture budget; the eager full-resolution path is removed | timeline/app thumbnail code | M | 3 |
| PB-009 | P1 | Complete | Persisted UI edits now declare and consume centralized texture, frame-thumbnail, and onion-skin effects | editor/cache invalidation | M | 2 |
| PB-010 | P1 | Complete | Layer operations expose current-frame or all-frame scope in their APIs and UI labels | editor state and layers UI | L | 2 |
| PB-011 | P2 | Complete | Tag, frame, layer, palette, and metadata counts/text are bounded in UI, model, encode, and decode paths | project metadata UI/model | S | 3 |
| PB-012 | P1 | Complete | Menu and shortcut Save share one command with encoding and async completion error handling | menu/save flow | M | 2 |
| PB-013 | P1 | Complete | Format, strict Clippy, 193 tests, native checks, and the WASM test build pass warning-free | workspace-wide | S | 0 |
| PB-014 | P2 | Complete | Shortcuts, dialogs, texture caches, app tests, and canvas tile layout are separated; fallible model construction/resize enforces core invariants | `src/app.rs`, editor/UI boundary | L | 4 |
| PB-015 | P3 | Complete | Dormant frame drag/drop code and unused direct TOML/clipboard dependencies are removed; direct dependencies and assets are reference-audited | frame drag/drop, `Cargo.toml` | S | 4 |
| PB-016 | P2 | Complete | Raster/project file bytes, decoded dimensions, and recovery snapshots have enforced early-rejection budgets | I/O and recovery paths | M | 3 |
| PB-017 | P0 | In progress | Effects now pause on the visible frame, run as a foreground modal, capture provenance, and reject stale Apply; remaining target-lock validation and full transaction regressions overlap PB-018 | effect lifecycle/modal/app coordination | M | 6 |
| PB-018 | P1 | Complete | Effects share one clipped Active Layer/Current Selection target; locked/missing layers, no-op commits, palette selection, selection-local geometry, shadow compositing, and pixel averaging enforce explicit contracts | effect transforms and commit boundary | L | 6 |
| PB-019 | P1 | Complete | Gradient inputs and stop counts are bounded, finite, endpoint-preserving, and sorted once per preview; sampling is allocation-free and visible pixels reach both ramp endpoints | `src/effects/gradient.rs`, effect UI | M | 6 |
| PB-020 | P2 | Open | New Effects growth recreated a monolithic UI/state/transform module and exposed additional model/API debt | `src/effects/mod.rs`, large coordinator/model files | L | 6 |

## Milestone 0 — Restore the engineering baseline

Goal: make the repository’s automated feedback trustworthy before behavioral changes continue.

Status: **Complete.** The newer Effects/Layers formatting and strict-Clippy regressions were repaired as the first Milestone 6 increment; all native and WASM gates are green again.

### Work

- [x] Apply `cargo fmt --all` and review the resulting diff for accidental semantic changes.
- [x] Make strict Clippy pass without broad `allow` attributes.
- [x] Resolve the dead `FrameDragPayload` and `frame_drop_destination` code:
  - finish and test the feature if it is intentionally retained, or
  - delete it and its now-unused plumbing.
- [x] Split or encapsulate `draw_layer_row_ui` instead of suppressing its excessive argument count.
- [x] Resolve the earlier collapsible-conditional diagnostic.
- [x] Fix the remaining module/test item-ordering diagnostic in `layers_panel.rs`.
- [x] Complete regression coverage for PB-001 through PB-005.
- [x] Record the supported Rust version as Rust 1.88 through `package.rust-version` in `Cargo.toml`; Milestone 5 will exercise it in CI.
- [x] Format the newer Effects/Layers changes and restore `cargo fmt --all -- --check`.
- [x] Fix the current `clone_on_copy` and `items_after_test_module` failures and restore strict Clippy.

### Exit gate

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo check --target wasm32-unknown-unknown --tests --all-features
```

Current result (2026-08-27): formatting, strict Clippy, 221/221 native tests, and the WASM test/application check all pass.

## Milestone 1 — Stop data loss and cross-document corruption

Goal: every document or frame transition is explicit, guarded, and covered by regression tests.

Status: **Complete.** PB-001 through PB-005 are implemented and the full native/WASM regression gates pass.

### 1.1 Unify new-document transitions — PB-001

- [x] Introduce one app-level command for replacing the active document.
- [x] Route New, settings presets, project open, image import, and recovery through that command.
- [x] Require dirty-document confirmation before destructive replacement.
- [x] Make the transition responsible for clearing or rebuilding history, autosave/recovery state, caches, selection, transient tools, and dialogs.

Acceptance criteria:

- Applying a preset to a dirty document cannot discard work without confirmation.
- Cancel leaves the current document byte-for-byte unchanged.
- Confirmed replacement starts with a clean history and no stale cached images.

Completion evidence (2026-08-20):

- Every destructive source now enters `request_document_replacement`; direct active-editor replacement remains only inside its commit path.
- Recovery uses the same dirty guard, and cancel retains both the current project and the recoverable snapshot.
- New/open projects finish clean; raster, sprite-sheet, and recovery projects finish dirty until saved.
- Commit clears history, selection, clipboards, playback, canvas transients, guides, content-dependent dialogs, recovery state, and all document-derived texture caches.
- Project/session and save-request IDs prevent delayed or out-of-order save completions, current-project imports, and inline layer-rename drafts from affecting a replacement project.
- Raster and sprite-sheet new projects start with default project preferences instead of inheriting serialized color/tool state from the discarded project.
- Regression coverage includes cancel/confirm behavior, stable pending targets, dirty/name policy, cache and dialog cleanup, guarded recovery, stale imports, stale and out-of-order save completion, and stale rename state.

Deferred product follow-up — palette policy for document replacement:

This was not implemented as part of PB-001. New raster and sprite-sheet projects currently use the one default palette so replacement behavior remains deterministic. The palette preset library and **Keep current / Use default / Choose preset** creation policy are now tracked in Milestone 6.1.

### 1.2 Route frame selection through `EditorState` — PB-002

- [x] Remove direct UI/app calls to low-level `animation.select_frame`.
- [x] Provide one `EditorState::select_frame` transition that commits or cancels transient edits, updates selection, clears frame-local history as designed, and invalidates dependent caches.
- [x] Route comma/period, timeline clicks, playback transitions, and programmatic selection through the shared editor/app transition commands.

Acceptance criteria:

- Undo immediately after switching frames cannot modify the previously selected frame or apply an old patch to the new one.
- Keyboard and pointer frame selection have identical state transitions.
- Boundary shortcuts do nothing safely at the first and last frames.

Completed 2026-08-20:

- Manual, timeline, playback, Stop, and structural frame transitions now share editor/app invariants for history, selections, canvas gestures, persisted playback selection, and rendering caches.
- Frame-bound async imports carry project/session, frame-generation, revision, and layer provenance; stale or partially valid targets are rejected atomically.
- Regression coverage includes no-wrap boundaries, cross-frame undo isolation, playback loops, visible-preview structural edits, staged timeline edits, stale thumbnails, and sprite-import playback/ABA/lock/no-op behavior.

### 1.3 Make dirty tracking complete — PB-004

Status: **Complete.** Persisted resize and all-frame layer topology changes now use editor/app mutation helpers with dirty and cache effects.

- [x] Inventory every document mutation and classify whether it changes persisted project state.
- [x] Route tag add, edit, and delete through committed actions that mark dirty only when the stored tag changes.
- [x] Mark global layer removal and the repaired palette/layer metadata paths dirty.
- [x] Route canvas resize and all-frame layer creation/duplication/removal through mutation helpers that mark dirty and invalidate every affected cache.
- [x] Keep tile configuration and animation-timeline visibility as separately persisted view preferences that do not change project bytes or revision.
- [x] Add dirty/revision/project-byte/cache regressions for the remaining resize and all-frame layer mutation categories; existing save/replacement tests cover saved-state and close guards.

Acceptance criteria:

- Closing after any persisted change prompts to save.
- A successful save clears dirty state; a failed or canceled save does not.
- Direct writes to persisted state outside the editor mutation boundary are eliminated or documented as temporary debt.

Completion evidence (2026-08-21):

- `EditorState` owns resize and all-frame add/duplicate/remove mutation helpers; each advances revision exactly once and clears invalid frame-local history.
- `PixelBuddyApp` applies the shared all-frame texture, thumbnail, onion-skin, pan, selection, and auto-fit effects.
- Both the Layers panel and Timeline call the same topology helpers instead of mutating animation frames directly.
- Remaining app-coordinator decomposition and stronger field privacy are tracked as PB-014 architectural debt, not as a known false-clean Milestone 1 path.

Suggested audit command:

```powershell
rg "editor\.(animation|layers|project|tags)|animation\." src
```

### 1.4 Make shortcuts focus-aware — PB-005

Status: **Complete.** One dispatcher now separates safe global commands from document commands and gates the latter on keyboard focus, modals, popups, and app dialogs.

- [x] Centralize global shortcut dispatch.
- [x] Suppress destructive/editing shortcuts while a text-edit widget wants keyboard input.
- [x] Preserve explicitly global commands only when their behavior is safe and conventional.
- [x] Block canvas pointer gestures behind the tag modal and its color-picker popup.
- [x] Test the shared focus predicate, a real focused text field, and foreground-dialog blocking; all text/numeric/tag/rename widgets consume the same egui keyboard-focus signal.

Acceptance criteria:

- Typing punctuation, letters, Backspace, or Delete in a text field never changes frames, tools, layers, or canvas contents.
- Modal dialogs prevent commands aimed at the document behind them.

### 1.5 Define and repair Merge Down — PB-003

Status: **Complete.** Index `0` is the bottom; Merge Down targets `active - 1` and commits one snapshot transaction.

- [x] Document the layer ordering contract: index `0` is the bottom layer.
- [x] Merge the active layer into the actual layer below it (`active - 1` under that contract).
- [x] Specify behavior for hidden, locked, opacity-adjusted, and blended layers.
- [x] Preserve the visual composite and choose the resulting layer name/metadata deterministically.
- [x] Make the whole operation one undoable transaction.

Acceptance criteria:

- Bottom-layer merge is disabled.
- Merge Down produces the same visible composite before and after the operation for supported blend modes.
- Undo/redo restores both pixels and layer metadata.
- Tests cover two layers, three layers, transparency, opacity, hidden/locked states, and boundary selection.

### Milestone 1 exit gate

- [x] All PB-001 through PB-005 regression tests pass.
- [x] No direct document/frame mutation remains in the identified Milestone 1 UI call sites.
- [x] Automated regressions cover create/draw/frame switch/undo-redo/replacement cancel-save behavior; repeat the equivalent manual UI smoke before a release build.

## Supplemental product and UI corrections completed

These changes were implemented during the audit because they affected correctness or made the repaired animation workflow usable. They do not close the remaining remediation findings by themselves.

- Native maximize/restore now distinguishes windowed, maximized, and fullscreen states; custom resize hit areas no longer cover screen-filling window controls.
- Tile preview supports configurable rows and columns, persists as view-only state, rejects interaction outside configured copies, handles seam-crossing strokes, and uses bounded low-zoom rendering.
- Animation tags support consecutive range selection, explicit create/edit dialogs, committed name/color/range changes, and stable membership when frames are inserted immediately after a range.
- Timeline cells now show cached 24×24 previews for each layer that actually exists; nonexistent layers do not receive cells or rows.
- Animation-timeline visibility is persisted as a view preference, and opening or recovering a multi-frame project automatically reopens it.
- The tag modal and color picker block raw canvas input, preventing clicks from leaking into the document.

## Milestone 2 — Establish one mutation and invalidation boundary

Goal: make incorrect dirty/history/cache behavior difficult to express in new code.

Status: **Complete.** Production UI mutations use scoped app/editor operations, edit consequences flow through a shared cache-effect consumer, and Save has one tested command path. PB-014 continues in Milestone 4 for coordinator decomposition and stronger field privacy.

### 2.1 Move persisted mutations behind editor methods — PB-009, PB-010, PB-014

- [x] Remove production UI writes through broad mutable model access; stronger field privacy remains PB-014 Milestone 4 work.
- [x] Add intention-revealing operations for layer metadata/topology, palette changes, tags, resize, animation settings, history navigation, and imports.
- [x] State scope in every layer operation name or type: current-frame versus all-frames.
- [x] Reject impossible, stale, unchanged, locked, or ambiguous operations at the API boundary.

### 2.2 Return explicit edit effects

Use a small result type so each mutation declares its consequences rather than relying on callers to remember them. For example:

```rust
struct EditEffects {
    document_changed: bool,
    history_changed: bool,
    current_texture_dirty: bool,
    frame_thumbnails_dirty: FrameSet,
    onion_skin_dirty: bool,
    recovery_dirty: bool,
}
```

The exact representation can differ, but the contract should be centralized and testable.

- [x] Consume edit effects in one app-level synchronization point.
- [x] Invalidate current, explicit non-current sets, all frames, or structural thumbnail collections.
- [x] Invalidate onion-skin inputs whenever artwork or frame structure can affect referenced neighbors.
- [x] Ensure multi-frame operations report every touched frame.

### 2.3 Consolidate commands and saving — PB-012

- [x] Define shared commands for New, Open, Save/Save As, imports, and destructive document replacement.
- [x] Route menus and keyboard shortcuts through those commands.
- [x] Preserve and surface serialization, encoding, and file-write errors.
- [x] Update filename and clean-state metadata only after a successful, current async write completion.

### Milestone 2 exit gate

- [x] UI modules request editor operations but do not directly mutate persisted models.
- [x] Menu and shortcut Save share one tested result path.
- [x] Thumbnail and onion-skin invalidation tests cover current, adjacent, explicit non-current, all-frame, and structural edits.
- [x] The scope of every layer command is visible in its API and UI label.

## Milestone 3 — Bound memory and untrusted input work

Goal: malformed or unusually large files fail early and predictably instead of exhausting memory or freezing the UI.

Status: **Complete.** PB-006, PB-007, PB-008, PB-011, and PB-016 have enforceable budgets and regression coverage.

### 3.1 Add sprite-sheet import budgets — PB-006

- [x] Define maximum source file bytes, decoded dimensions, aggregate decoded pixels, tile count, and imported frame count.
- [x] Inspect headers/dimensions before a full decode where the image library permits it.
- [x] Validate tile dimensions and calculated frame count with checked arithmetic.
- [x] Reject invalid imports before constructing all frame buffers.
- [x] Generate a bounded, downsampled preview instead of decoding or uploading a full-size preview texture.
- [x] Return specific user-facing errors for each exceeded limit.

Implemented limits: 64 MiB encoded raster input, 8,192 pixels per dimension, 16,777,216 pixels per decoded canvas, 1,024 imported sprite frames, 8,388,608 aggregate imported frame pixels, and a 256×256 maximum preview texture. Tests cover zero and overflow-shaped grids, extreme aspect ratios, excessive frame/pixel counts, truncated data, bounded previews, and normal imports.

### 3.2 Make thumbnails proportional to their display size — PB-008

Status: **Complete.** Timeline previews are fixed-size and lazy; the eager full-resolution composite upload path has been removed.

- [x] Generate per-layer timeline previews at a fixed 24×24 pixel budget.
- [x] Cache layer previews by document session, editor revision, frame/layer structure, and nearest-neighbor texture parameters.
- [x] Render previews only for layers that actually exist in each frame.
- [x] Remove the legacy full-resolution `frame_thumbnails` texture allocation path.
- [x] Build previews lazily only for visible timeline cells.
- [x] Evict old GPU thumbnails with a documented 512-texture LRU budget.

The remaining `frame_thumbnails` vector is invalidation bookkeeping only and no production path populates it with textures. Regressions prove main-texture updates allocate no frame thumbnails and the timeline cache evicts the least-recently-used entry at its cap.

### 3.3 Add a byte budget to history — PB-007

- [x] Conservatively estimate each history entry's retained bytes, including canvas pixels, layer names, palettes, and suspended futures.
- [x] Cap both combined entry count and total retained bytes.
- [x] Keep sparse pixel edits as patches and admit structural snapshots only when they fit the budget.
- [x] Drop the oldest complete transactions when the budget is exceeded.
- [x] Keep the 64 MiB safety ceiling fixed because there is not yet a useful user-facing memory/performance tradeoff.
- [x] Stress repeated fills and oversized structural snapshots; existing import and multi-frame structural regressions exercise the same snapshot path.

Undo, redo, and suspended replacement branches share one 64 MiB retained-memory budget. An edit still applies when its undo snapshot is individually oversized, but that snapshot is not retained, preventing an unbounded history allocation.

### 3.4 Bound project and recovery inputs — PB-011, PB-016

- [x] Bound tag count and tag byte/character length; reject control characters and invalid color/range metadata.
- [x] Apply file-size, decoded-dimension, frame, layer, palette, and text limits consistently to project and image/import paths. PixelBuddy has no standalone palette-file import path.
- [x] Reject oversized native files from filesystem metadata before reading them; WASM and decoder paths enforce the same byte limit after receipt and before decode.
- [x] Define a 32 MiB maximum recovery snapshot and discard over-budget snapshots without parsing or crashing.
- [x] Update recovery as one storage value and verify empty, over-budget, truncated, and corrupt restore inputs.

Project files are capped at 256 MiB and project canvases at 256 MiB of decoded pixel data. Projects also cap animation frames at 4,096, layers per frame and palette colors at 256, tags at 1,024, tag names at 64 characters/128 bytes, and layer names at 256 bytes. Encode and decode share the same metadata validation, so invalid in-memory state cannot be written and invalid files cannot enter the editor.

### Milestone 3 exit gate

- [x] Adversarial import tests fail with bounded memory and clear errors.
- [x] Large-document stress tests demonstrate bounded history and thumbnail memory.
- [x] No integer multiplication used for allocation sizing is unchecked; remaining multiplication audit hits are bounded indexing or test assertions.
- [x] Project metadata and recovery files have documented, enforced limits.

## Milestone 4 — Reduce architectural friction and dead code

Goal: shrink the surface where future changes can bypass established invariants.

Status: **Complete for the audited scope.** PB-014 and PB-015 remain closed. Subsequent Effects development increased several large files again; that new structural debt is tracked separately as PB-020 rather than retroactively reopening the original extraction work.

### 4.1 Decompose the app coordinator — PB-014

- [x] Move shortcut recognition into a typed, model-independent `ShortcutDispatcher`; `PixelBuddyApp` executes commands but no longer interprets raw key combinations.
- [x] Move lifecycle, recovery, close/replacement, sprite-import, and export dialogs into `app/dialogs.rs`.
- [x] Move canvas, checkerboard, and onion-skin texture cache management into `app/textures.rs`.
- [x] Move the app regression suite to `app/tests.rs` so production coordination code is not buried under test fixtures.
- [x] Replace the nine-argument layer-row function with one scoped `LayerRowUi` request that exposes only its target and staged outputs.
- [x] Split canvas tile layout/hit-testing and the canvas regression suite into `ui/canvas_view/` submodules.
- [x] Keep document/editor models independent of egui and preserve behavior through the full regression suite.

Historical largest-file evidence: `src/app.rs` fell from roughly 5,030 to 2,502 lines, and `src/ui/canvas_view.rs` fell from roughly 2,077 to 1,421 lines. Current growth has brought `app.rs` to roughly 2,864 lines, `canvas_view.rs` to 1,523 lines, and the new `effects/mod.rs` to roughly 1,850 lines. The earlier extractions remain cohesive, but Effects now needs its own state/transform/UI split under PB-020.

### 4.2 Strengthen model invariants

- [x] Add validated fallible `Canvas`, `Layer`, and `Document` construction; internal infallible conveniences now fail fast instead of creating inconsistent empty canvases.
- [x] Make dimension-dependent resize allocation fallible and atomic across every layer and animation frame.
- [x] Prove an invalid resize preserves project bytes, pixels, revision, history state, and clean/dirty state.
- [x] Evaluate stable frame/layer IDs. Current session, revision, and frame-generation provenance remains sufficient after dormant drag/reorder UI removal; introduce stable IDs if persistent external references or richer reorder workflows return.
- [x] Document the top-left coordinate system, bottom-to-top layer order, positional frame identity, and unpremultiplied source-over alpha contract near their model types.

### 4.3 Remove unused code and dependencies — PB-015

- [x] Delete the dormant `FrameDragPayload`, `frame_drop_destination`, and tests that covered only that unreachable helper.
- [x] Run a direct-dependency usage audit and confirm the resulting native/WASM dependency tree.
- [x] Remove unused direct `toml` and `arboard` declarations; `arboard` remains only as an expected eframe transitive dependency.
- [x] Reference-check every bundled asset (22/22 referenced) and confirm there are no custom feature flags or additional orphaned assets.
- [x] Clear the former dead-code, excessive-argument, and item-ordering warnings without broad `allow` attributes.

### Milestone 4 exit gate

- [x] Core construction and resize invariants are enforced below the UI layer.
- [x] `PixelBuddyApp` delegates raw shortcuts, dialogs, and texture-cache responsibilities to focused modules.
- [x] Native and WASM builds are warning-free under strict Clippy.
- [x] No confirmed unused direct dependencies, feature flags, assets, or orphaned internal frame-DnD APIs remain.

## Milestone 5 — Harden the release pipeline

Goal: make dependency, supply-chain, licensing, and platform checks repeatable.

Status: **Implementation complete; first pushed clean-runner confirmation pending.** The workflow defines the documented Rust 1.88 native/WASM gates, but the current local format/Clippy regression must be fixed before the first push can produce a clean result.

- [x] Add `cargo audit` or `cargo deny` to a scheduled and pull-request workflow.
- [x] Review the transitive `ttf-parser 0.25.1` unmaintained advisory; monitor upstream because the audit found no patched release at the time of writing.
- [x] Pin GitHub Actions to immutable commit SHAs and use dependency update automation to refresh them.
- [x] Generate and review license/dependency notices, including bundled fonts and visual assets.
- [x] Configure CI to test the documented minimum supported Rust version, native release build/smoke test, and WASM test/application build.
- [x] Add fixtures for corrupt projects, oversized metadata, truncated recovery data, and hostile sprite sheets.
- [x] Document security/resource limits and the recovery-file lifecycle.

### Milestone 5 exit gate

- [x] CI defines format, lint, tests, native build/smoke, WASM build/smoke, dependency audit, and license checks.
- [x] Actions are SHA-pinned and covered by Dependabot.
- [x] Known advisories have a documented disposition.
- [ ] Release artifacts pass the first pushed clean hosted-runner smoke test; local `main` is 27 commits ahead of `origin/main`, and current formatting/Clippy failures would block the quality job.

## Milestone 6 — Complete palette, color, and raster-effects workflows

Goal: fill the major art-workflow gaps without weakening the mutation, undo, cache, input-isolation, or resource-limit boundaries established in Milestones 1–4.

Status: **In progress; safety hardening required before more feature work.** The larger RGB/hex picker is complete. A built-in palette library and current/default/preset replacement policy are present. All thirteen planned effects are exposed with live-preview prototypes, including Palettize, Outline, Drop Shadow, Pixelize, Gradient Fill, and Gradient Map. These newer implementations do not yet satisfy the established modal/provenance, target, lock, no-op, palette, gradient-performance, or regression contracts; PB-017 through PB-020 are open.

Recent UI resilience work also replaced the fixed 200 px right sidebar with a responsive 240 px default that users can resize from 220–320 px. The Blend Mode label and selector now stack vertically and the palette grid continues deriving its columns from available width, preventing DPI/window-width clipping from recurring as a one-off row fix.

### Rough implementation guidance

- **Palette presets:** keep preset definitions in a small model-only module with stable IDs and validated color arrays. Represent the user's choice as a `PalettePolicy` carried inside every pending replacement payload, then resolve and apply it only when replacement is confirmed. Reuse one swatch-preview widget in New Project and new-project import flows.
- **Effect scope and behavior:** introduce shared types such as `EffectTarget` and `EdgeMode` instead of adding unrelated booleans to each dialog. Resolve active-layer/selection scope once before building a preview, reject locked or empty targets early, and show the chosen scope and edge behavior directly in each modal.
- **Advanced pixel effects:** implement Outline, Drop Shadow, Palettize, and Pixelize as pure document/canvas transforms independent of egui. Give each transform a validated parameter struct and deterministic output, then connect it to the existing preview document and one-transaction Apply path.
- **Palettize:** reuse the preset library, begin with one documented color-distance metric, preserve alpha, and add dithering only as an explicit option. Keep palette selection separate from project-creation palette policy even if both share the same preset data.
- **Gradients:** build one reusable `ColorRamp` model and rasterizer first. Validate ordered stops and geometry before preview generation, then layer the interactive stop editor, interpolation/color-space options, linear/radial geometry, repeat modes, and dithering UI on top. Gradient Fill and Gradient Map should share the ramp while remaining separate transforms.
- **Preview performance:** rebuild previews only when parameters change, keep the project document immutable, and reuse bounded textures. For neighborhood effects such as outlines and shadows, cap radius/blur work and avoid allocating more than a small number of canvas-sized scratch buffers.
- **Testing:** use table-driven pure-transform tests for pixel output and app-level tests for opening, changing, applying, canceling, undo/redo, dirty state, selection clipping, locked layers, and cache invalidation. Add native/WASM output hashes for deterministic transforms and boundary tests for the largest accepted parameters.
- **Warning cleanup:** format the newer Effects/Layers changes, replace the copied `Selection::clone()` with its `Copy` value, move `layers_panel` production items before its test module, and restore the complete format/strict-Clippy gate before further feature work.

#### Shared preview performance contract

Apply these rules to every current and future effect, filter, transform, or dialog that renders a live document preview:

- [x] Keep one effect-owned preview document and restore its active-layer pixel buffer in place when dimensions and layer structure are unchanged. Do not clone the entire project on every parameter tick.
- [x] Commit the already-rendered preview on **Apply** as one editor mutation. Do not run the transform a second time or let the committed result differ from the final visible preview.
- [x] Limit continuous pointer-driven preview refreshes to 30 Hz while still forcing an immediate refresh after release or any non-drag parameter change.
- [x] Cache repeated Adjust Color RGB conversions with a bounded 4,096-entry per-refresh cache; preserve each pixel's original alpha.
- [x] Use bulk wrapped row copies for whole-layer Offset previews, while retaining the exact per-pixel path when a selection constrains the operation.
- [x] Reuse the preview's active-layer allocation when possible and keep temporary full-canvas storage to a small, documented number of buffers.
- [x] Cover allocation reuse, optimized-vs-reference pixel output, cache-equivalent color output, and drag/final-refresh timing with focused regressions.
- [ ] If a future effect still misses interaction targets on the largest supported canvases, add a bounded downsampled interaction preview and/or a cancellable background worker with generation IDs. Always finish with an exact full-resolution preview before Apply becomes available.
- [ ] For neighborhood effects, gradients, and other multi-pass operations, reuse scratch buffers and define explicit work limits before exposing radius, blur, stop-count, or iteration controls.


Suggested rough order from the current state:

1. Restore formatting and strict Clippy.
2. Fix PB-017 so an effect is a true document/frame-bound modal transaction that pauses playback and blocks surrounding mutations.
3. Fix PB-018 effect target, locked-layer, no-op, selection-boundary, palette-index, and alpha contracts.
4. Fix PB-019 gradient validation, targeting, endpoint correctness, and per-pixel allocation/sort behavior.
5. Complete and test the palette-policy edge cases and visible secondary-color UI.
6. Split the Effects monolith and finish the remaining transform-quality policies under PB-020.
7. Run the full undo/cache/resource/native/WASM regression and performance pass, then push for hosted CI confirmation.

### 6.1 Add project-creation palette presets

- [x] Define a built-in palette library with stable identifiers, display names, ordered opaque colors, and one explicit default palette.
- [x] Add a palette choice to New Project and new-project raster/sprite-sheet imports: **Keep current palette**, **Use default palette**, or **Choose preset**.
- [x] Show a swatch preview and palette name before a pending destructive replacement is confirmed.
- [x] Carry the chosen palette policy inside the pending replacement payload so it cannot change behind the confirmation dialog.
- [x] Apply the palette only after replacement commit; cancellation leaves the active project byte-for-byte unchanged.
- [ ] Reject empty, oversized, or otherwise invalid presets and fall back deterministically to the default palette if a stored preset identifier no longer exists.
- [ ] Make **Keep current palette** actually preserve the discarded project's palette. The current commit sequence replaces the editor first, so the no-op Keep Current branch retains the new editor's default/import palette instead.
- [ ] Keep palette selection scoped to project creation: it must not silently change unrelated primary/secondary colors, tools, view preferences, or an existing project.
- [ ] Test all three policies across New, raster import, sprite-sheet import, dirty-project cancel/confirm, and missing-preset fallback paths.

Acceptance criteria:

- A user can deliberately start with the current palette, PixelBuddy's default palette, or a named built-in preset.
- The selection visible in the creation flow is exactly the palette committed to the replacement project.
- Palette policy never bypasses the shared destructive-replacement guard.

### 6.2 Redesign the primary color picker

- [x] Replace the current compact popup with a larger fixed-footprint picker modeled on the provided reference: a substantially larger saturation/value field, a clear hue strip, color preview, and input row.
- [x] Remove the **Saturation and value** hover tooltip; the field should be self-explanatory and must not create a label overlapping the popup.
- [x] Give the popup explicit minimum and maximum dimensions and keep it inside the viewport by flipping or clamping its anchor near screen edges.
- [x] Give the RGB controls fixed widths so values from 0 through 255 never widen the popup or push it off-screen.
- [x] Preserve the existing gamma-byte RGB inputs and their two-way synchronization with the saturation/value and hue controls.
- [x] Add an editable six-digit hexadecimal field accepting `#RRGGBB` and `RRGGBB`, case-insensitively, and normalize valid values to one canonical display form.
- [x] Keep colors opaque for this first iteration; do not imply `RRGGBBAA` support until palette alpha behavior is designed.
- [x] Invalid or incomplete hex input must not mutate the selected color, resize the popup, close it, or leak a click to the canvas; show a compact inline invalid state instead.
- [x] Preserve popup keyboard/click isolation, Escape and click-outside dismissal, copy behavior, gray-color hue retention, and RGB/HSV/hex synchronization.
- [x] Add pure parsing/formatting tests plus constrained-width, three-digit RGB, invalid-hex, viewport-edge, and canvas click-through regressions.
- [ ] Expose the editor's secondary color in the main UI beside the primary color. Make the active swatch unambiguous and allow both colors to use the same RGB/HSV/hex picker, because new gradients initialize from the primary-to-secondary color pair.

Acceptance criteria:

- Changing any RGB, hex, hue, saturation, or value control immediately updates every other representation to the same opaque color.
- The popup dimensions remain stable for every valid numeric and hex value.
- Opening or using the picker over the canvas cannot draw, dismiss prematurely, or move the underlying document.
- The primary and secondary colors are both visible and editable, and a newly opened gradient starts with the same two colors shown by those swatches.

### 6.3 Add the missing Effects menu and raster operations

Before implementing individual effects:

- [x] Add an **Effects** menu and a shared effect request/preview/commit boundary rather than letting menu callbacks mutate canvases directly.
- [ ] Decide and label each effect's scope explicitly: active layer, current selection when present, current frame, or all animation frames. Do not infer an all-frame edit from an unlabeled command.
- [ ] Define transparent-edge, alpha, selection-boundary, locked-layer, and empty-selection behavior for every effect.
- [x] Keep preview pixels in an effect-owned preview document rather than `EditorState`; opening, adjusting, or canceling an effect leaves project bytes, revision, dirty state, and recovery input unchanged. Apply remains the only editor mutation boundary.
- [x] Bound the shared modal preview to an aspect-correct 320×200 display area and retain the existing checked canvas/project limits for effect source documents.
- [ ] Keep native and WASM output deterministic and cover cancel, no-op, selection, locked-layer, undo/redo, cache effects, and limit rejection.

Missing effect inventory from the reference:

- [x] **Offset Image (baseline)** — signed X/Y translation with wrap behavior and live preview; explicit edge-mode labeling remains part of the scope/contract pass.
- [x] **Mirror Image (baseline)** — horizontal and vertical reflection with live preview; explicit pivot/scope labeling remains open.
- [x] **Rotate Image (baseline)** — readable 0°, 90°, 180°, and 270° presets plus an editable −180°…180° angle; fixed-canvas, center-based nearest-neighbor sampling keeps pixel output crisp and previews non-destructively.
- [x] **Outline (prototype)** — color, thickness, outside Manhattan-distance outline, and live preview are present; inside/outside and transparent-edge policies remain open under PB-018.
- [x] **Drop Shadow (prototype)** — color, opacity, signed offset, and live preview are present; correct normalized-alpha compositing and a bounded blur policy remain open under PB-018.
- [x] **Invert Colors** — define whether transparent RGB channels are preserved and keep alpha unchanged.
- [x] **Desaturation** — deterministic luminance conversion with alpha unchanged.
- [x] **Adjust Color** — previewable hue, saturation, and brightness/value controls using the dialog contract in 6.3.1.
- [x] **Palettize (prototype)** — current/default/preset mapping with Euclidean RGB distance is present; palette-index repair, a documented distance policy, swatch preview, and optional dithering remain open.
- [x] **Pixelize (prototype)** — configurable averaged block size and live preview are present; alpha-weighting, alignment anchor, and sampling policy remain open.
- [x] **Posterize** — configurable per-channel or luminance level count with stable quantization.
- [x] **Gradient Fill (prototype)** — linear/radial generation, bounded stops, interpolation, color processing, edge modes, dithering, Replace/Alpha Blend, shared targeting, validation, and allocation-free sampling are present; direct canvas handle interaction and the broad exit-gate matrix remain.
- [x] **Gradient Map (prototype)** — luminance mapping through the shared validated ramp preserves alpha and obeys the unified target region; the broad exit-gate matrix remains.

#### 6.3.1 Color adjustment dialog

Use PixelBuddy-oriented labels rather than copying the reference dialog literally:

- [x] Open **Adjust Color…** as a foreground modal with the shared bounded live preview of the affected artwork.
- [ ] Provide synchronized sliders and fixed-width numeric fields for **Hue Shift**, **Saturation**, and **Brightness/Value**; changing any control must update the preview immediately without resizing the dialog.
- [x] Use explicit neutral values and bounded ranges, with a **Reset** action that returns all three adjustments to zero without closing the dialog.
- [x] Preserve alpha and fully transparent pixel data unless a later effect explicitly advertises otherwise.
- [x] Offer one **Active Layer** or **Current Selection** target control. Selection targeting is available only for a non-empty clipped selection and never affects or samples pixels outside it.
- [x] Keep the source project unchanged while previewing. The effect-owned preview document is rendered by the main canvas and modal preview; **Apply** alone enters the editor mutation path, while **Cancel**, Escape, and close discard preview state.
- [x] Use a true modal/provenance boundary that blocks canvas tools, shortcuts, playback, frame/layer controls, and other UI mutations; project replacement clears the transaction and stale provenance rejects Apply.
- [ ] Test positive/negative range boundaries, zero/no-op, selection clipping, transparent pixels, numeric-field width, Apply/Cancel, undo/redo, and native/WASM color parity.

Suggested user-facing names are **Adjust Color**, **Hue Shift**, **Saturation**, **Brightness/Value**, **Target**, **Reset**, **Apply**, and **Cancel**. These preserve the reference capabilities without copying its labels or layout one-for-one.

#### 6.3.2 Gradient editor

- [x] Open **Gradient Fill…** as a foreground modal with a bounded live preview of the target pixels.
- [x] Provide an editable color ramp with 2–32 stops. Users can add, move, recolor, and remove interior stops while endpoints remain fixed and valid.
- [x] Reuse the Milestone 6.2 RGB/hex picker for stop colors so RGB, hex, hue, saturation, and value stay synchronized.
- [x] Add a **Distribute Stops Evenly** action for multi-stop ramps.
- [x] Offer interpolation choices with PixelBuddy names **Step**, **Linear**, and **Smooth**; Smooth uses cubic-style easing.
- [x] Offer color-processing choices **sRGB** and **Linear RGB**, with a visible default; deterministic native/WASM parity still needs the exit-gate tests.
- [x] Support **Linear** and **Radial** shapes with numeric start/end and center/radius controls.
- [ ] Allow radial axes to be linked for a circle or unlinked for an ellipse, and provide a canvas interaction for positioning the center/endpoints without leaking edits to the document.
- [x] Offer edge/repetition behavior using **Clamp**, **Repeat**, and **Mirror**.
- [x] Offer dithering choices **None**, **Bayer 2×2**, and **Bayer 4×4**; target clipping and native/WASM regressions remain open.
- [x] Use the shared **Active Layer** or **Current Selection** target; selection mode is disabled when no clipped selection exists.
- [x] Provide explicit **Replace** and **Alpha Blend** modes for Gradient Fill.
- [x] Validate stop count, finite normalized geometry, radii, and percentages before transform output; enum-typed interpolation/repeat modes cannot represent invalid variants.
- [x] Keep the source unchanged during preview. **Apply** creates one undoable dirty edit unless byte-identical; **Cancel**, Escape, close, or invalid input restores exact pre-dialog bytes.
- [x] Share the ramp editor with **Gradient Map** while keeping Fill geometry generation distinct from luminance mapping with preserved alpha.
- [ ] Test stop editing, even distribution, interpolation/color-space modes, linear/radial geometry, linked radii, repeat modes, dithering, selection clipping, blend modes, invalid parameters, Apply/Cancel, undo/redo, cache effects, and native/WASM parity.

Suggested user-facing names are **Gradient Fill**, **Color Ramp**, **Interpolation**, **Color Processing**, **Shape**, **Dithering**, **Edge Mode**, **Center**, **Radius**, **Target**, **Distribute Stops**, **Apply**, and **Cancel**.

### 6.4 Repair Effects transaction safety and correctness — PB-017, PB-018, PB-019

Treat every effect as a document/frame-bound transaction, not merely a floating preview window:

- [x] Pause playback and adopt the visible editing frame before capturing an effect source.
- [x] Capture document session, active-frame generation, editor revision, active layer, and selection provenance with the effect state.
- [x] Replace the ordinary effect `egui::Window` with a true foreground modal; shortcuts, playback, canvas input, and surrounding controls cannot mutate the document behind it.
- [x] Reject and cancel Apply if its captured document/frame/layer/selection provenance is no longer current; project replacement clears any active effect.
- [x] Reject locked or missing target layers before preview allocation and revalidate immediately before commit.
- [x] Replace the duplicated general/gradient target state with one explicit **Active Layer** or **Current Selection** control. Disable Selection when no non-empty clipped selection exists.
- [x] Define selection-local geometry for Offset, Mirror, Rotate, Outline, Drop Shadow, Pixelize, Fill, and Map so pixels outside the target cannot influence or receive the transform unexpectedly.
- [x] Detect byte-identical previews and return a true no-op: no revision, history entry, dirty state, cache invalidation, or recovery churn.
- [x] Clamp the selected palette index whenever Palettize replaces the palette; fall back to the default palette if malformed runtime state supplies an empty target.
- [x] Normalize Drop Shadow alpha compositing through the shared Porter-Duff source-over implementation; chosen shadow-color alpha multiplies source alpha and the opacity control.
- [x] Make Pixelize averaging alpha-aware so transparent RGB does not darken or tint partially covered blocks.
- [x] Bound gradient stop count at 32, preserve fixed endpoints, reject non-finite/out-of-range parameters, and use total ordering.
- [x] Sort and validate gradient stops once per preview refresh rather than cloning and sorting the stop vector for every output pixel.
- [x] Map normalized gradient endpoints to the first and last visible target pixels so a 0→1 ramp displays both endpoint colors.
- [ ] Add regressions for playback advance, frame/layer/project changes behind the modal, locked layers, stale provenance, selection/no-selection targets, neutral Apply, palette validity, alpha edges, endpoint colors, maximum stop count, and native/WASM parity.

### 6.5 Consolidate code quality and modularity — PB-020

The earlier Milestone 4 extractions remain valuable, but the current largest files are again substantial: `app.rs` ≈2,864 lines, `effects/mod.rs` ≈1,850, `app/tests.rs` ≈2,085, `timeline_panel.rs` ≈1,341, `canvas_view.rs` ≈1,523, `layers_panel.rs` ≈1,310, and `io/project.rs` ≈1,324.

- [ ] Split `effects/mod.rs` into focused state/parameter, pure-transform, preview/transaction, and egui UI modules. Transform modules must not depend on egui.
- [ ] Split effect regressions by transform and transaction behavior instead of continuing to grow one inline test module.
- [ ] Extract remaining app playback/document-command coordination when touched, without recreating broad mutable access to editor internals.
- [ ] Split `app/tests.rs` by subsystem and separate project encode/decode validation if those files continue growing.
- [ ] Replace history front-removal loops with a queue-oriented representation or equivalent O(1) oldest-entry eviction while preserving suspended-future semantics and byte budgets.
- [ ] Make normalized layer opacity and active-layer selection harder to bypass through public fields; route changes through validated methods.
- [ ] Enforce `MAX_LAYERS_PER_FRAME` in the model-level layer-add operation, not only app/UI callers.
- [ ] Decide and document transform-quality policies before changing output compatibility: Posterize bucket formula, rounding versus truncation, Palettize RGB versus perceptual distance, and Pixelize sampling/alpha behavior.
- [ ] Keep the warning, dependency, and asset audits green as modules move; do not use broad lint suppressions to hide structural regressions.

Recommended implementation order:

1. [x] Shared effect preview/commit API and Effects menu shell.
2. [x] Lossless geometry: Offset, Mirror, and right-angle Rotate.
3. [x] Deterministic color transforms: Invert, Desaturation, Adjust Color, and Posterize.
4. [ ] Close PB-017 effect modal/provenance corruption paths.
5. [x] Close PB-018 target, lock, no-op, palette, alpha, and selection contracts.
6. [x] Close PB-019 gradient correctness, validation, and performance defects.
7. [ ] Complete Keep Current palette behavior and the visible secondary-color workflow.
8. [ ] Complete PB-020 Effects decomposition and transform-quality decisions.
9. [ ] Run the full native/WASM performance, undo, cache, and hostile-parameter regression pass.

### Milestone 6 exit gate

- [ ] Project creation offers tested current/default/preset palette policies.
- [x] The larger RGB/hex picker is fixed-size, viewport-safe, and cannot leak input to the canvas.
- [ ] Every listed effect has an explicit scope and alpha/edge contract.
- [ ] Every effect previews safely, commits as one undoable mutation, and participates in dirty/cache/recovery handling.
- [ ] Native and WASM checks remain warning-free and the full effect/resource regression suite passes.
## Final release gate

Do not treat the remediation program as complete until all of the following are true:

- [ ] No P0 or P1 finding is open without an explicit, time-bounded risk acceptance.
- [ ] Dirty-document confirmation is consistent across every replacement path.
- [ ] Undo/redo cannot cross frame or document boundaries incorrectly.
- [ ] Every persisted mutation participates in dirty tracking and cache invalidation.
- [ ] Text input and modal focus suppress unsafe global shortcuts.
- [ ] Imports, history, thumbnails, project metadata, and recovery data have enforceable memory/size budgets.
- [ ] Formatting, strict Clippy, tests, native checks, and WASM checks are green in CI.
- [ ] The dead-code and direct-dependency audits are clean.
- [ ] Manual smoke tests cover create, edit, layer operations, animation, save/open, import/export, recovery, and WASM behavior.

## Recommended implementation sequence

The next work remains ordered by user/data risk. Keep each behavioral change independently reviewable and include its regression tests.

Completed:

1. [x] Unified destructive document replacement and recovery guard — PB-001.
2. [x] Unified manual, timeline, playback, and structural frame transitions — PB-002.

Milestone 1 completion:

3. [x] Finished persisted-mutation dirty tracking for resize and all-frame layer topology — PB-004.
4. [x] Made global shortcuts focus- and modal-aware — PB-005.
5. [x] Defined and repaired Merge Down as one undoable compositing transaction — PB-003.

Immediate next:
6. [x] Close the warning-free baseline: remove dead frame drag/drop, encapsulate `draw_layer_row_ui`, fix module ordering, and pass strict Clippy — PB-013/PB-015.
7. [x] Unified menu/shortcut Save and propagated encoding/write failures — PB-012.
8. [x] Introduced explicit mutation effects and completed cache invalidation/layer-scope contracts — PB-009/PB-010; PB-014 decomposition continues in Milestone 4.
9. [x] Replace the remaining eager full-resolution thumbnail cache — PB-008.
10. [x] Add sprite-sheet, project, tag, and recovery resource budgets — PB-006/PB-011/PB-016.
11. [x] Add a byte budget to history — PB-007.
12. [x] Decompose the largest coordinator/UI files and remove unused direct dependencies — PB-014/PB-015. CI and release hardening continue in Milestone 5.
13. [x] Harden the release pipeline with MSRV/native/WASM gates, SHA-pinned Actions, cargo-deny, notices, fixtures, and recovery/security documentation; hosted smoke confirmation follows the first push.
14. [ ] Finish Milestone 6: repair effect transaction safety, complete palette/secondary-color edge cases, harden every transform contract, and modularize the Effects implementation — PB-017 through PB-020.

The palette-policy chooser deferred from Milestone 1.1 now exists, but Keep Current behavior and policy regressions remain open. PB-017 is the immediate priority because it can apply an effect preview to the wrong frame or overwrite newer document state.

## Progress log

| Date | Milestone / area | Status | Evidence / notes |
|---|---|---|---|
| 2026-08-20 | Baseline audit | Complete | 90 tests passed; WASM check passed with two dead-code warnings; formatting and strict Clippy did not pass; `cargo-audit` was unavailable locally |
| 2026-08-20 | Milestone 1.1 / PB-001 | Complete | Unified guarded replacement, recovery, session/save provenance, state/cache/dialog cleanup, and replacement policy tests |
| 2026-08-20 | Milestone 1.2 / PB-002 | Complete | Shared frame-transition invariants for keyboard, timeline, playback, Stop, structural edits, and async import provenance |
| 2026-08-20 | Window and tile-preview reliability | Complete for current scope | Native restore/fullscreen behavior repaired; configurable view-only tile preview, bounded ruler/preview rendering, seam-safe editing, and persistence added |
| 2026-08-21 | Animation timeline tags | Complete for current scope | Consecutive selection, explicit create/edit modal, color/input isolation, exact range commits, and insertion-stable tag membership |
| 2026-08-21 | Timeline layer previews | Complete | Per-layer 24×24 revision-aware thumbnails render only for real visible cells; a 512-texture LRU replaces the eager full-resolution composite cache |
| 2026-08-21 | Timeline visibility persistence | Complete | View preference survives restart; loaded/recovered multi-frame projects reopen the timeline without changing project bytes |
| 2026-08-21 | Milestone 0 / PB-013 | Complete | Format, strict warning-free Clippy, 193 tests, native checks, and the WASM test build pass |
| 2026-08-21 | Milestone 1.3 / PB-004 | Complete | Resize and all-frame layer topology now use dirty/cache-aware mutation helpers with byte/revision/cache regressions |
| 2026-08-21 | Milestone 1.4 / PB-005 | Complete | Central dispatcher gates document shortcuts for focused text, modals, popups, and foreground dialogs |
| 2026-08-21 | Milestone 1.5 / PB-003 | Complete | Merge Down targets active - 1, safely rejects hidden/locked/non-Normal cases, preserves supported composites, and restores metadata through undo/redo |
| 2026-08-21 | Milestone 1 exit gate | Complete | 176 native tests pass; WASM test build and format pass; no new Clippy warnings |
| 2026-08-21 | Milestone 2 / PB-009, PB-010, PB-012 | Complete | Scoped UI/editor commands, centralized edit effects, selective thumbnail/onion invalidation, all-frame layer contracts, and one failure-safe Save command; 179 native tests and WASM test build pass |
| 2026-08-21 | Milestone 3 / PB-006, PB-007, PB-008, PB-011, PB-016 | Complete | Early input/project/recovery budgets, metadata limits, a shared 64 MiB history ceiling, lazy 24×24 thumbnails with a 512-texture LRU, checked allocation sizing, 193 native tests, and WASM test build pass |
| 2026-08-21 | Milestone 4 / PB-014, PB-015 | Complete | `app.rs` 5,030→2,502 lines; `canvas_view.rs` 2,077→1,421; typed shortcut, dialog, texture, tile-layout, and test modules; atomic fallible resize; dead-code/dependency/asset audit clean; strict native/WASM gates and 193 tests pass |
| 2026-08-21 | Milestone 5 / release pipeline | Implementation complete; hosted confirmation pending | SHA-pinned Actions and Dependabot; Rust 1.88 format/strict Clippy/195 tests/native release/WASM pass; cargo-deny advisories/licenses/sources pass with one documented unmaintained transitive exception; deterministic notices, hostile fixtures, security/recovery docs, and native/Web smoke checks added |
| 2026-08-21 | Milestone 6 / palette, picker, and effects roadmap | In progress | Larger fixed RGB/hex picker complete; built-in preset and replacement-policy infrastructure added; thirteen-effect inventory and detailed Adjust Color/Gradient contracts retained |
| 2026-08-21 | Responsive right sidebar | Complete for current scope | Replaced fixed 200 px width with a resizable 220–320 px policy (240 px default), stacked Blend Mode controls, and retained width-derived palette columns; full 193-test suite passed at implementation time |
| 2026-08-21 | Arbitrary canvas dimensions | Complete for current scope | Settings now exposes validated Custom Size flows for New Canvas and Resize Existing Canvas; both retain the 8,192-pixel side and 16,777,216-pixel aggregate safety limits while allowing non-preset dimensions |
| 2026-08-21 | Milestone 6 / Shared preview performance | Complete for current synchronous effects | Reused active-layer preview buffers, committed the visible preview without recomputation, added bounded Adjust Color caching and bulk wrapped Offset copies, and capped pointer-driven refresh at 30 Hz with immediate final refresh; the reusable escalation path for downsampling/workers is documented above |
| 2026-08-21 | Milestone 6 / Effects foundation and Rotate | Superseded by current Milestone 6 status | The original seven-effect foundation expanded to all thirteen planned prototypes; later audit found the window is not a true modal and opened PB-017 through PB-020 |
| 2026-08-25 | Roadmap reconciliation and current baseline | Active / blockers recorded | 206 native tests and WASM test build pass; formatting and strict Clippy regressions remain. Palette/effect prototypes were reconciled with actual code, PB-017 through PB-020 were added, and valid code-quality work was consolidated into this canonical roadmap |
| 2026-08-25 | Milestone 6 / PB-017 and PB-018 transaction boundary | In progress | Effects pause and adopt the visible playback frame, capture project/frame/revision/layer/selection provenance, use a true foreground modal, cancel stale Apply, clear on project replacement, reject locked/missing layers, and commit byte-identical previews as clean no-ops. Format, strict Clippy, 210 tests, and WASM checks pass |
| 2026-08-27 | Milestone 6 / PB-018 and PB-019 effect contracts | Complete | Unified Active Layer/Current Selection targeting, selection-local geometry, palette-index repair, normalized shadow alpha, alpha-weighted Pixelize, bounded and allocation-free gradient sampling, fixed visible endpoints, invalid-gradient rejection, and project-replacement transaction coverage; format, strict Clippy, 221 tests, and WASM checks pass |

Update this table whenever a finding starts, changes scope, or closes. Link a commit or pull request when the local 27-commit remediation series is pushed or reorganized into reviewable remote changes.
