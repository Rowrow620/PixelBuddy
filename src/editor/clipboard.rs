use crate::document::Document;
use crate::editor::selection::Selection;

#[derive(Clone, Debug)]
pub struct ClipboardBuffer {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl ClipboardBuffer {
    pub fn copy(doc: &Document, selection: &Selection) -> Option<Self> {
        let active_layer = doc.active_layer();
        let (min_x, max_x, min_y, max_y) = if selection.active {
            (
                selection.min_x().max(0) as u32,
                (selection.max_x()).min(doc.width as i32 - 1) as u32,
                selection.min_y().max(0) as u32,
                (selection.max_y()).min(doc.height as i32 - 1) as u32,
            )
        } else {
            (
                0,
                doc.width.saturating_sub(1),
                0,
                doc.height.saturating_sub(1),
            )
        };

        if min_x > max_x || min_y > max_y {
            return None;
        }

        let w = max_x - min_x + 1;
        let h = max_y - min_y + 1;
        let mut pixels = Vec::with_capacity((w * h) as usize);

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                pixels.push(active_layer.canvas.get_pixel(x, y));
            }
        }

        Some(Self {
            width: w,
            height: h,
            pixels,
        })
    }
}
