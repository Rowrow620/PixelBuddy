#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Selection {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub active: bool,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
            active: false,
        }
    }

    pub fn set_rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.x0 = x0.min(x1);
        self.y0 = y0.min(y1);
        self.x1 = x0.max(x1);
        self.y1 = y0.max(y1);
        self.active = true;
    }

    pub fn deselect(&mut self) {
        self.active = false;
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        if !self.active {
            return false;
        }
        x >= self.min_x() && x <= self.max_x() && y >= self.min_y() && y <= self.max_y()
    }

    pub fn min_x(&self) -> i32 { self.x0.min(self.x1) }
    pub fn max_x(&self) -> i32 { self.x0.max(self.x1) }
    pub fn min_y(&self) -> i32 { self.y0.min(self.y1) }
    pub fn max_y(&self) -> i32 { self.y0.max(self.y1) }
    pub fn width(&self) -> u32 { (self.max_x() - self.min_x() + 1).max(1) as u32 }
    pub fn height(&self) -> u32 { (self.max_y() - self.min_y() + 1).max(1) as u32 }
}
