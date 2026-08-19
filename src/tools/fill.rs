use super::PixelChange;
use crate::document::canvas::Canvas;
use std::collections::VecDeque;

fn color_match(a: [u8; 4], b: [u8; 4], tolerance: u8) -> bool {
    a.iter().zip(b.iter()).all(|(c1, c2)| {
        let diff = c1.abs_diff(*c2);
        diff <= tolerance
    })
}

/// Flood fills a region with the specified color.
pub fn flood_fill(
    canvas: &Canvas,
    start_x: u32,
    start_y: u32,
    fill_color: [u8; 4],
    tolerance: u8,
    contiguous: bool,
) -> Vec<PixelChange> {
    let mut changes = Vec::new();
    if start_x >= canvas.width() || start_y >= canvas.height() {
        return changes;
    }

    let target_color = canvas.get_pixel(start_x, start_y);
    if color_match(target_color, fill_color, 0) {
        return changes;
    }

    if contiguous {
        let w = canvas.width() as usize;
        let h = canvas.height() as usize;
        let mut visited = vec![false; w * h];
        let mut queue = VecDeque::new();

        queue.push_back((start_x, start_y));
        visited[(start_y as usize) * w + (start_x as usize)] = true;

        let dirs = [(0, 1), (1, 0), (0, -1), (-1, 0)];

        while let Some((x, y)) = queue.pop_front() {
            changes.push((x, y, fill_color));

            for &(dx, dy) in &dirs {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;

                if nx >= 0 && nx < canvas.width() as i32 && ny >= 0 && ny < canvas.height() as i32 {
                    let nu_x = nx as u32;
                    let nu_y = ny as u32;
                    let idx = (nu_y as usize) * w + (nu_x as usize);

                    if !visited[idx] {
                        let c = canvas.get_pixel(nu_x, nu_y);
                        if color_match(c, target_color, tolerance) {
                            visited[idx] = true;
                            queue.push_back((nu_x, nu_y));
                        }
                    }
                }
            }
        }
    } else {
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                let c = canvas.get_pixel(x, y);
                if color_match(c, target_color, tolerance) {
                    changes.push((x, y, fill_color));
                }
            }
        }
    }

    changes
}
