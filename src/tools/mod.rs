pub mod eraser;
pub mod eyedropper;
pub mod fill;
pub mod line;
pub mod marquee;
pub mod move_tool;
pub mod pencil;
pub mod shape;

/// Represents a single pixel change: (x, y, new_color).
pub type PixelChange = (u32, u32, [u8; 4]);
