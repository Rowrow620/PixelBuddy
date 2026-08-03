use super::PixelChange;
use super::line::bresenham_line;

/// Draws a single pixel at the specified position.
pub fn draw_pixel(x: u32, y: u32, color: [u8; 4]) -> Vec<PixelChange> {
    vec![(x, y, color)]
}

/// Draws a stroke through a series of points, producing a pixel-perfect line.
pub fn draw_stroke(points: &[(u32, u32)], color: [u8; 4]) -> Vec<PixelChange> {
    if points.is_empty() {
        return Vec::new();
    }
    
    let mut raw_points = Vec::new();
    for i in 0..points.len() - 1 {
        let (x0, y0) = points[i];
        let (x1, y1) = points[i + 1];
        let mut line_pts = bresenham_line(x0 as i32, y0 as i32, x1 as i32, y1 as i32);
        // avoid duplicating points at the joints
        if i < points.len() - 1 {
            line_pts.pop(); 
        }
        raw_points.extend(line_pts);
    }
    if let Some(&last) = points.last() {
        raw_points.push((last.0 as i32, last.1 as i32));
    }

    // Apply pixel-perfect algorithm: remove corners in 2x2 blocks
    let mut filtered_points = Vec::new();
    if !raw_points.is_empty() {
        filtered_points.push(raw_points[0]);
    }
    
    for i in 1..raw_points.len() {
        let prev = filtered_points.last().unwrap();
        let curr = raw_points[i];
        
        if i + 1 < raw_points.len() {
            let next = raw_points[i + 1];
            if prev.0 != next.0 && prev.1 != next.1 && 
               ((curr.0 == prev.0 && curr.1 == next.1) || (curr.0 == next.0 && curr.1 == prev.1)) {
                // curr is a corner pixel, skip it
                continue;
            }
        }
        // Deduplicate
        if *prev != curr {
            filtered_points.push(curr);
        }
    }

    filtered_points
        .into_iter()
        .filter(|&(x, y)| x >= 0 && y >= 0)
        .map(|(x, y)| (x as u32, y as u32, color))
        .collect()
}
