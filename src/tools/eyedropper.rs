use crate::document::canvas::Canvas;

/// Returns the color at the specified canvas coordinates.
pub fn pick_color(canvas: &Canvas, x: u32, y: u32) -> [u8; 4] {
    if x < canvas.width() && y < canvas.height() {
        canvas.get_pixel(x, y)
    } else {
        [0, 0, 0, 0]
    }
}
