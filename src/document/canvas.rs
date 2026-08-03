#[derive(Clone, Debug)]
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        }
    }

    pub fn new_with_color(width: u32, height: u32, color: [u8; 4]) -> Self {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&color);
        }
        Self { width, height, pixels }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if !self.in_bounds(x as i32, y as i32) {
            return [0, 0, 0, 0];
        }
        let idx = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ]
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if !self.in_bounds(x as i32, y as i32) {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.pixels[idx] = color[0];
        self.pixels[idx + 1] = color[1];
        self.pixels[idx + 2] = color[2];
        self.pixels[idx + 3] = color[3];
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < self.width as i32 && y >= 0 && y < self.height as i32
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn clear(&mut self, color: [u8; 4]) {
        for chunk in self.pixels.chunks_mut(4) {
            chunk.copy_from_slice(&color);
        }
    }

    pub fn blend_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) {
        if !self.in_bounds(x as i32, y as i32) {
            return;
        }
        let base = self.get_pixel(x, y);
        let alpha = color[3] as f32 / 255.0;
        let inv_alpha = 1.0 - alpha;
        
        let out_alpha = alpha + (base[3] as f32 / 255.0) * inv_alpha;
        if out_alpha == 0.0 {
            self.set_pixel(x, y, [0, 0, 0, 0]);
            return;
        }
        
        let out_r = (color[0] as f32 * alpha + base[0] as f32 * (base[3] as f32 / 255.0) * inv_alpha) / out_alpha;
        let out_g = (color[1] as f32 * alpha + base[1] as f32 * (base[3] as f32 / 255.0) * inv_alpha) / out_alpha;
        let out_b = (color[2] as f32 * alpha + base[2] as f32 * (base[3] as f32 / 255.0) * inv_alpha) / out_alpha;
        
        self.set_pixel(x, y, [
            out_r.round() as u8,
            out_g.round() as u8,
            out_b.round() as u8,
            (out_alpha * 255.0).round() as u8,
        ]);
    }
}
