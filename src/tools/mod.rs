pub mod pencil;
pub mod eraser;
pub mod line;
pub mod shape;
pub mod fill;
pub mod eyedropper;

/// Represents a single pixel change: (x, y, new_color).
pub type PixelChange = (u32, u32, [u8; 4]);
