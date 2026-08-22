#[derive(Clone, Debug)]
pub struct Palette {
    pub colors: Vec<[u8; 4]>,
    pub selected_index: usize,
}

impl Default for Palette {
    fn default() -> Self {
        crate::document::palette_library::default_preset().to_palette()
    }
}

impl Palette {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_color(&mut self, color: [u8; 4]) {
        self.colors.push(color);
        self.selected_index = self.colors.len() - 1;
    }

    /// Removes a color without allowing the palette to become empty.
    ///
    /// Returns whether a color was removed. If the selected color was not the
    /// one removed, it remains selected even when its index changes.
    pub fn remove_color(&mut self, index: usize) -> bool {
        if self.colors.len() <= 1 || index >= self.colors.len() {
            return false;
        }

        self.clamp_selected_index();
        self.colors.remove(index);

        if self.selected_index > index {
            self.selected_index -= 1;
        } else if self.selected_index >= self.colors.len() {
            // Removing the last selected color selects its new predecessor.
            self.selected_index = self.colors.len() - 1;
        }

        true
    }

    /// Moves a palette color to another existing palette slot.
    ///
    /// Returns whether the palette changed. The selected color remains
    /// selected, rather than merely retaining its old numeric index.
    pub fn move_color(&mut self, from: usize, to: usize) -> bool {
        if from >= self.colors.len() || to >= self.colors.len() || from == to {
            return false;
        }

        self.clamp_selected_index();
        let color = self.colors.remove(from);
        self.colors.insert(to, color);

        if self.selected_index == from {
            self.selected_index = to;
        } else if from < self.selected_index && self.selected_index <= to {
            self.selected_index -= 1;
        } else if to <= self.selected_index && self.selected_index < from {
            self.selected_index += 1;
        }

        true
    }

    pub fn selected_color(&self) -> [u8; 4] {
        if let Some(&color) = self.colors.get(self.selected_index) {
            color
        } else {
            [0, 0, 0, 255]
        }
    }

    pub fn set_selected(&mut self, index: usize) {
        if index < self.colors.len() {
            self.selected_index = index;
        }
    }

    fn clamp_selected_index(&mut self) {
        debug_assert!(!self.colors.is_empty());
        self.selected_index = self.selected_index.min(self.colors.len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::Palette;

    fn color(value: u8) -> [u8; 4] {
        [value, 0, 0, 255]
    }

    fn palette(colors: &[u8], selected_index: usize) -> Palette {
        Palette {
            colors: colors.iter().copied().map(color).collect(),
            selected_index,
        }
    }

    #[test]
    fn removing_a_color_keeps_a_later_selected_color_selected() {
        let mut palette = palette(&[1, 2, 3], 2);

        assert!(palette.remove_color(0));

        assert_eq!(palette.colors, vec![color(2), color(3)]);
        assert_eq!(palette.selected_index, 1);
        assert_eq!(palette.selected_color(), color(3));
    }

    #[test]
    fn removing_the_selected_color_chooses_its_successor_or_predecessor() {
        let mut palette = palette(&[1, 2, 3], 1);

        assert!(palette.remove_color(1));
        assert_eq!(palette.selected_color(), color(3));
        assert_eq!(palette.selected_index, 1);

        assert!(palette.remove_color(1));
        assert_eq!(palette.selected_color(), color(1));
        assert_eq!(palette.selected_index, 0);
    }

    #[test]
    fn removing_the_last_color_or_an_invalid_slot_does_nothing() {
        let mut single_color_palette = palette(&[1], 0);
        assert!(!single_color_palette.remove_color(0));
        assert_eq!(single_color_palette.colors, vec![color(1)]);

        let mut palette = palette(&[1, 2], 0);
        assert!(!palette.remove_color(2));
        assert_eq!(palette.colors, vec![color(1), color(2)]);
    }

    #[test]
    fn moving_a_color_preserves_the_selected_color() {
        let mut palette = palette(&[1, 2, 3, 4], 1);

        assert!(palette.move_color(1, 3));

        assert_eq!(palette.colors, vec![color(1), color(3), color(4), color(2)]);
        assert_eq!(palette.selected_index, 3);
        assert_eq!(palette.selected_color(), color(2));
    }

    #[test]
    fn moving_another_color_adjusts_the_selected_index() {
        let mut palette = palette(&[1, 2, 3, 4], 2);

        assert!(palette.move_color(0, 3));
        assert_eq!(palette.selected_index, 1);
        assert_eq!(palette.selected_color(), color(3));

        assert!(palette.move_color(3, 0));
        assert_eq!(palette.selected_index, 2);
        assert_eq!(palette.selected_color(), color(3));
    }

    #[test]
    fn moving_to_the_same_or_an_invalid_slot_does_nothing() {
        let mut palette = palette(&[1, 2], 0);

        assert!(!palette.move_color(0, 0));
        assert!(!palette.move_color(0, 2));
        assert!(!palette.move_color(2, 0));
        assert_eq!(palette.colors, vec![color(1), color(2)]);
        assert_eq!(palette.selected_index, 0);
    }
}
