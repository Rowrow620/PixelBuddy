use super::PixelChange;

/// Generates points along a line using Bresenham's line algorithm.
pub fn bresenham_line(mut x0: i32, mut y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let mut points = Vec::new();
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        points.push((x0, y0));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }

    points
}

/// Draws a line between two points with the specified color.
pub fn draw_line(x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) -> Vec<PixelChange> {
    bresenham_line(x0, y0, x1, y1)
        .into_iter()
        .filter(|&(x, y)| x >= 0 && y >= 0)
        .map(|(x, y)| (x as u32, y as u32, color))
        .collect()
}
