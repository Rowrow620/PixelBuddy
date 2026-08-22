# Hostile input fixtures

These files are intentionally invalid or over-budget and must never be opened as trusted project data:

- `corrupt_project.pbud`: invalid RON syntax.
- `oversized_metadata.pbud`: structurally valid project data with a layer name beyond the UTF-8 byte limit.
- `truncated_recovery.pbud`: a cut-off recovery/project value.
- `hostile_spritesheet.png`: a truncated PNG header advertising an over-limit dimension.

Unit tests load these bytes with `include_str!`/`include_bytes!` so the exact hostile samples ship with the regression suite without runtime filesystem access.
