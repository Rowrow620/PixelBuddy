#[derive(Clone, Debug)]
pub struct Palette {
    pub colors: Vec<[u8; 4]>,
    pub selected_index: usize,
}

impl Default for Palette {
    fn default() -> Self {
        let colors = vec![
            [0, 0, 0, 255],       // Black
            [255, 255, 255, 255], // White
            [128, 128, 128, 255], // Gray
            [192, 192, 192, 255], // Light Gray
            [255, 0, 0, 255],     // Red
            [0, 255, 0, 255],     // Green
            [0, 0, 255, 255],     // Blue
            [255, 255, 0, 255],   // Yellow
            [0, 255, 255, 255],   // Cyan
            [255, 0, 255, 255],   // Magenta
            [255, 165, 0, 255],   // Orange
            [128, 0, 128, 255],   // Purple
            [139, 69, 19, 255],   // Saddle Brown
            [255, 192, 203, 255], // Pink
            [255, 218, 185, 255], // Peach
            [160, 82, 45, 255],   // Sienna
            [34, 139, 34, 255],   // Forest Green
            [0, 128, 128, 255],   // Teal
            [75, 0, 130, 255],    // Indigo
            [210, 180, 140, 255], // Tan
            [255, 140, 0, 255],   // Dark Orange
            [173, 216, 230, 255], // Light Blue
            [240, 128, 128, 255], // Light Coral
            [152, 251, 152, 255], // Pale Green
        ];
        
        Self {
            colors,
            selected_index: 0,
        }
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
    
    pub fn remove_color(&mut self, index: usize) {
        if index < self.colors.len() && self.colors.len() > 1 {
            self.colors.remove(index);
            if self.selected_index >= self.colors.len() {
                self.selected_index = self.colors.len() - 1;
            }
        }
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
}
