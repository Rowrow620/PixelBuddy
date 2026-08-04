use crate::editor::selection::Selection;

pub fn update_selection(selection: &mut Selection, x0: i32, y0: i32, x1: i32, y1: i32) {
    selection.set_rect(x0, y0, x1, y1);
}
