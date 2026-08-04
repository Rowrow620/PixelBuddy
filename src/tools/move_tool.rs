use crate::document::Document;
use crate::editor::selection::Selection;
use crate::tools::PixelChange;

pub fn move_pixels(doc: &Document, selection: &Selection, dx: i32, dy: i32) -> Vec<PixelChange> {
    if dx == 0 && dy == 0 {
        return Vec::new();
    }

    let active_layer = doc.active_layer();
    let width = doc.width as i32;
    let height = doc.height as i32;

    let mut changes = Vec::new();

    let (min_x, max_x, min_y, max_y) = if selection.active {
        (
            selection.min_x().max(0),
            selection.max_x().min(width - 1),
            selection.min_y().max(0),
            selection.max_y().min(height - 1),
        )
    } else {
        (0, width - 1, 0, height - 1)
    };

    if min_x > max_x || min_y > max_y {
        return Vec::new();
    }

    // Erase original locations
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            changes.push((x as u32, y as u32, [0, 0, 0, 0]));
        }
    }

    // Place shifted pixels
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let new_x = x + dx;
            let new_y = y + dy;
            if new_x >= 0 && new_x < width && new_y >= 0 && new_y < height {
                let color = active_layer.canvas.get_pixel(x as u32, y as u32);
                changes.push((new_x as u32, new_y as u32, color));
            }
        }
    }

    changes
}
