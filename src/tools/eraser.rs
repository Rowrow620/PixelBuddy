use super::PixelChange;
use super::pencil::{draw_pixel, draw_stroke};

const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

/// Erases a single pixel at the specified position (sets it to transparent).
pub fn erase_pixel(x: u32, y: u32) -> Vec<PixelChange> {
    draw_pixel(x, y, TRANSPARENT)
}

/// Erases along a stroke through a series of points using Bresenham line.
pub fn erase_stroke(points: &[(u32, u32)]) -> Vec<PixelChange> {
    draw_stroke(points, TRANSPARENT)
}
